use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Product {
    pub folder: &'static str,
    pub image: &'static str,
    pub service: &'static [u8],
}

pub(super) const NAVO: Product = Product {
    folder: rusty_core::brand::NAME,
    image: "navo.exe",
    service: b"Navo Index",
};

pub(super) const FERRITE: Product = Product {
    folder: rusty_core::brand::LEGACY_NAME,
    image: "ferrite.exe",
    service: b"Ferrite Index",
};

impl Product {
    pub fn data_directories(&self, program_data: &Path) -> [PathBuf; 2] {
        let parent = program_data.join(self.folder);
        let index = parent.join("index");
        [parent, index]
    }
}

pub(super) struct CopyPlan {
    pub directory: PathBuf,
    pub temporary: PathBuf,
    pub executable: PathBuf,
    pub previous: PathBuf,
    image: &'static str,
}

impl CopyPlan {
    pub fn new(program_files: &Path, nonce: u32) -> Self {
        Self::of(NAVO, program_files, nonce)
    }

    pub fn of(product: Product, program_files: &Path, nonce: u32) -> Self {
        let directory = program_files.join(product.folder).join("Index");
        let image = product.image;
        Self {
            temporary: directory.join(format!("{image}.{nonce}.tmp")),
            executable: directory.join(image),
            previous: directory.join(format!("{image}.{nonce}.old")),
            directory,
            image,
        }
    }

    pub fn copy_from(&self, source: &Path) -> io::Result<()> {
        let mut target = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.temporary)?;
        let result = (|| {
            let mut source = std::fs::File::open(source)?;
            io::copy(&mut source, &mut target)?;
            target.sync_all()
        })();
        if result.is_err() {
            drop(target);
            let _ = std::fs::remove_file(&self.temporary);
        }
        result
    }

    pub fn rename_old(&self) -> io::Result<()> {
        match std::fs::rename(&self.executable, &self.previous) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    pub fn replace(&self) -> io::Result<()> {
        std::fs::rename(&self.temporary, &self.executable)
    }

    pub fn restore(&self) -> io::Result<()> {
        let cleanup = self.discard_copy();
        if !self.previous.try_exists()? {
            return cleanup;
        }
        retry(RESTORE_TRIES, RESTORE_DELAY, || {
            std::fs::rename(&self.previous, &self.executable)
        })?;
        cleanup
    }

    pub fn discard_copy(&self) -> io::Result<()> {
        match std::fs::remove_file(&self.temporary) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    pub fn recover_old(&self) -> io::Result<()> {
        match std::fs::symlink_metadata(&self.executable) {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let mut previous = None;
        for entry in std::fs::read_dir(&self.directory)? {
            let entry = entry?;
            if !is_old_image(self.image, &entry.file_name().to_string_lossy()) {
                continue;
            }
            if !entry.file_type()?.is_file() || previous.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "previous service image is ambiguous",
                ));
            }
            previous = Some(entry.path());
        }
        if let Some(previous) = previous {
            retry(RESTORE_TRIES, RESTORE_DELAY, || {
                std::fs::rename(&previous, &self.executable)
            })?;
        }
        Ok(())
    }

    pub fn delete_old(&self) -> io::Result<()> {
        let mut result = Ok(());
        for entry in std::fs::read_dir(&self.directory)? {
            let entry = entry?;
            if is_old_image(self.image, &entry.file_name().to_string_lossy()) {
                result = result.and(std::fs::remove_file(entry.path()));
            }
        }
        result
    }
}

const RESTORE_TRIES: u32 = 5;
const RESTORE_DELAY: Duration = Duration::from_millis(200);

pub(super) fn retry<T>(
    tries: u32,
    delay: Duration,
    mut operation: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    let mut attempt = 1;
    loop {
        match operation() {
            Err(_) if attempt < tries => {
                attempt += 1;
                std::thread::sleep(delay);
            }
            result => return result,
        }
    }
}

pub(super) struct SettingsBackup {
    path: PathBuf,
    previous: Option<Vec<u8>>,
}

impl SettingsBackup {
    pub fn capture(path: &Path) -> io::Result<Self> {
        let previous = match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        Ok(Self {
            path: path.to_path_buf(),
            previous,
        })
    }

    pub fn restore(&self) -> io::Result<()> {
        match &self.previous {
            Some(bytes) => rusty_store::write_atomic(&self.path, bytes),
            None => match std::fs::remove_file(&self.path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                result => result,
            },
        }
    }
}

pub(super) struct FreshInstall {
    had_executable: bool,
    missing: Vec<PathBuf>,
}

impl FreshInstall {
    pub fn observe(plan: &CopyPlan, directories: &[PathBuf]) -> Self {
        Self {
            had_executable: plan.executable.exists(),
            missing: directories
                .iter()
                .filter(|path| !path.exists())
                .cloned()
                .collect(),
        }
    }

    pub fn recover_executable(&mut self, plan: &CopyPlan) -> io::Result<()> {
        plan.recover_old()?;
        self.observe_executable(plan);
        Ok(())
    }

    pub fn observe_executable(&mut self, plan: &CopyPlan) {
        self.had_executable = plan.executable.exists();
    }

    pub fn remove_executable(&self, plan: &CopyPlan) -> io::Result<()> {
        if self.had_executable {
            return Ok(());
        }
        match std::fs::remove_file(&plan.executable) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    pub fn remove_directories(&self) -> io::Result<()> {
        let mut result = Ok(());
        for path in self.missing.iter().rev() {
            match std::fs::remove_dir(path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                other => result = result.and(other),
            }
        }
        result
    }
}

fn is_old_image(image: &str, name: &str) -> bool {
    name.strip_prefix(image)
        .and_then(|name| name.strip_prefix('.'))
        .and_then(|name| name.strip_suffix(".old"))
        .is_some_and(|nonce| !nonce.is_empty() && nonce.bytes().all(|byte| byte.is_ascii_digit()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    RecoverOld,
    Stop,
    Copy,
    RenameOld,
    Replace,
    BackupSettings,
    Configure,
    Start,
    Restore,
    RestoreSettings,
    RemoveFresh,
    DiscardCopy,
    RestoreConfiguration,
    DeleteOld,
}

pub(super) fn install_steps(exists: bool) -> &'static [Step] {
    use Step::*;
    if exists {
        &[
            Copy,
            RenameOld,
            Replace,
            Stop,
            BackupSettings,
            Configure,
            Start,
        ]
    } else {
        &[Copy, RenameOld, Replace, BackupSettings, Configure, Start]
    }
}

pub(super) fn run_install(
    exists: bool,
    was_running: bool,
    mut execute: impl FnMut(Step) -> io::Result<()>,
) -> io::Result<()> {
    execute(Step::RecoverOld)?;
    let _ = execute(Step::DeleteOld);
    let mut completed = Vec::new();
    for &step in install_steps(exists) {
        if let Err(error) = execute(step) {
            return Err(recover((exists, was_running), error, &completed, execute));
        }
        completed.push(step);
    }
    let _ = execute(Step::DeleteOld);
    Ok(())
}

fn recover(
    (exists, was_running): (bool, bool),
    error: io::Error,
    completed: &[Step],
    mut execute: impl FnMut(Step) -> io::Result<()>,
) -> io::Error {
    let mut failures = Vec::new();
    let replaced = completed.contains(&Step::Replace);
    let mut rollback = Vec::new();
    if exists && replaced {
        rollback.push((Step::Stop, "stopping the replacement service"));
    }
    if completed.contains(&Step::BackupSettings) {
        rollback.push((Step::RestoreSettings, "restoring service.json"));
    }
    if completed.contains(&Step::RenameOld) {
        rollback.push((Step::Restore, "restoring the previous executable"));
    } else if completed.contains(&Step::Copy) {
        rollback.push((Step::DiscardCopy, "discarding the temporary copy"));
    }
    if !exists && replaced {
        rollback.push((Step::RemoveFresh, "removing the new executable"));
    }
    if exists && replaced {
        rollback.push((
            Step::RestoreConfiguration,
            "restoring service configuration",
        ));
    }
    if exists && was_running {
        rollback.push((Step::Start, "restarting the service"));
    }
    for (step, context) in rollback {
        if let Err(error) = execute(step) {
            failures.push(format!("{context}: {error}"));
        }
    }
    if failures.is_empty() {
        return error;
    }
    io::Error::new(error.kind(), format!("{error}; {}", failures.join("; ")))
}

pub(super) fn data_directories(program_data: &Path) -> [PathBuf; 2] {
    NAVO.data_directories(program_data)
}

pub(super) fn clear_directory(path: &Path) -> io::Result<()> {
    let mut result = Ok(());
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let (kind, path) = (entry.file_type()?, entry.path());
        result = result.and(if kind.is_symlink() {
            std::fs::remove_file(&path).or_else(|_| std::fs::remove_dir(&path))
        } else if kind.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_is_copied_to_a_temporary_name_then_renamed_in_program_files() {
        let plan = CopyPlan::new(Path::new("program-files"), 123);
        assert_eq!(plan.directory, Path::new("program-files/Navo/Index"));
        assert_eq!(plan.executable, plan.directory.join("navo.exe"));
        assert_eq!(plan.temporary.parent(), Some(plan.directory.as_path()));
        assert_ne!(plan.temporary, plan.executable);
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("portable.exe");
        let plan = CopyPlan::new(root.path(), 123);
        std::fs::create_dir_all(&plan.directory).unwrap();
        std::fs::write(&source, b"new binary").unwrap();
        std::fs::write(&plan.executable, b"old binary").unwrap();
        plan.copy_from(&source).unwrap();
        plan.rename_old().unwrap();
        plan.replace().unwrap();
        assert_eq!(std::fs::read(&plan.executable).unwrap(), b"new binary");
        assert!(!plan.temporary.exists());
    }

    #[test]
    fn reinstall_moves_the_new_image_in_before_it_stops_the_service() {
        use Step::*;
        assert_eq!(
            install_steps(false),
            &[Copy, RenameOld, Replace, BackupSettings, Configure, Start]
        );
        assert_eq!(
            install_steps(true),
            &[
                Copy,
                RenameOld,
                Replace,
                Stop,
                BackupSettings,
                Configure,
                Start
            ]
        );
    }

    #[test]
    fn copy_refuses_an_existing_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let plan = CopyPlan::new(root.path(), 1);
        std::fs::create_dir_all(&plan.directory).unwrap();
        std::fs::write(&plan.temporary, b"untouched").unwrap();
        assert!(plan.copy_from(&plan.temporary).is_err());
        assert_eq!(std::fs::read(&plan.temporary).unwrap(), b"untouched");
    }
}
