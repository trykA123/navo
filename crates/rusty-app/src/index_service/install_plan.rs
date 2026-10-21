use std::io;
use std::path::{Path, PathBuf};

pub(super) struct CopyPlan {
    pub directory: PathBuf,
    pub temporary: PathBuf,
    pub executable: PathBuf,
    pub previous: PathBuf,
}

impl CopyPlan {
    pub fn new(program_files: &Path, nonce: u32) -> Self {
        let directory = program_files.join("Ferrite").join("Index");
        Self {
            temporary: directory.join(format!("ferrite.exe.{nonce}.tmp")),
            executable: directory.join("ferrite.exe"),
            previous: directory.join(format!("ferrite.exe.{nonce}.old")),
            directory,
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
        let _ = std::fs::remove_file(&self.temporary);
        if self.executable.try_exists()? || !self.previous.try_exists()? {
            return Ok(());
        }
        std::fs::rename(&self.previous, &self.executable)
    }

    pub fn delete_old(&self) -> io::Result<()> {
        let mut result = Ok(());
        for entry in std::fs::read_dir(&self.directory)? {
            let entry = entry?;
            if is_old_image(&entry.file_name().to_string_lossy()) {
                result = result.and(std::fs::remove_file(entry.path()));
            }
        }
        result
    }
}

fn is_old_image(name: &str) -> bool {
    name.strip_prefix("ferrite.exe.")
        .and_then(|name| name.strip_suffix(".old"))
        .is_some_and(|nonce| !nonce.is_empty() && nonce.bytes().all(|byte| byte.is_ascii_digit()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Stop,
    Copy,
    RenameOld,
    Replace,
    Configure,
    Start,
    Restore,
    DeleteOld,
}

pub(super) fn install_steps(exists: bool) -> &'static [Step] {
    use Step::*;
    if exists {
        &[Copy, RenameOld, Replace, Stop, Configure, Start]
    } else {
        &[Copy, RenameOld, Replace, Configure, Start]
    }
}

pub(super) fn run_install(
    exists: bool,
    mut execute: impl FnMut(Step) -> io::Result<()>,
) -> io::Result<()> {
    let _ = execute(Step::DeleteOld);
    match install_steps(exists)
        .iter()
        .try_for_each(|&step| execute(step))
    {
        Ok(()) => {
            let _ = execute(Step::DeleteOld);
            Ok(())
        }
        Err(error) => Err(recover(exists, error, execute)),
    }
}

fn recover(
    exists: bool,
    error: io::Error,
    mut execute: impl FnMut(Step) -> io::Result<()>,
) -> io::Error {
    let mut failures = Vec::new();
    if let Err(restore) = execute(Step::Restore) {
        failures.push(format!("restoring the previous executable: {restore}"));
    }
    if exists && let Err(start) = execute(Step::Start) {
        failures.push(format!("restarting the service: {start}"));
    }
    if failures.is_empty() {
        return error;
    }
    io::Error::new(error.kind(), format!("{error}; {}", failures.join("; ")))
}

pub(super) fn data_directories(program_data: &Path) -> [PathBuf; 2] {
    let parent = program_data.join("Ferrite");
    let index = parent.join("index");
    [parent, index]
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
        assert_eq!(plan.directory, Path::new("program-files/Ferrite/Index"));
        assert_eq!(plan.executable, plan.directory.join("ferrite.exe"));
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
            &[Copy, RenameOld, Replace, Configure, Start]
        );
        assert_eq!(
            install_steps(true),
            &[Copy, RenameOld, Replace, Stop, Configure, Start]
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
