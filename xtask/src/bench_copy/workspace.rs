use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) struct Workspace {
    pub source: PathBuf,
    pub target: PathBuf,
    restore_target: bool,
    source_owned: bool,
    target_owned: bool,
}

impl Workspace {
    pub fn new(target: &Path) -> Result<Self, String> {
        let name = target
            .file_name()
            .ok_or_else(|| "target must name a folder, not a filesystem root".to_owned())?;
        let parent = target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let parent = fs::canonicalize(parent)
            .map(|path| simplified(&path))
            .map_err(|error| format!("{}: {error}", parent.display()))?;
        let target = parent.join(name);
        let restore_target = empty_target(&target)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let source = parent.join(format!(
            ".navo-bench-source-{}-{timestamp}",
            std::process::id()
        ));
        if source == target {
            return Err("source and target folders must differ".to_owned());
        }
        Ok(Self {
            source,
            target,
            restore_target,
            source_owned: false,
            target_owned: false,
        })
    }

    pub fn prepare(&mut self) -> Result<(), String> {
        if empty_target(&self.target)? != self.restore_target {
            return Err("target changed before benchmark setup".to_owned());
        }
        if !self.restore_target {
            create_folder(&self.target)?;
        }
        self.target_owned = true;
        create_folder(&self.source)?;
        self.source_owned = true;
        Ok(())
    }

    pub fn reset_target(&self) -> Result<(), String> {
        remove_folder(&self.target)?;
        create_folder(&self.target)
    }

    pub fn clean_up(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        if self.source_owned {
            collect_error(remove_folder(&self.source), &mut errors);
        }
        if self.target_owned {
            collect_error(self.clean_target(), &mut errors);
        }
        if errors.is_empty() {
            self.source_owned = false;
            self.target_owned = false;
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn clean_target(&self) -> Result<(), String> {
        remove_folder(&self.target)?;
        if self.restore_target {
            create_folder(&self.target)?;
        }
        Ok(())
    }
}

pub(super) fn simplified(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

pub(super) fn empty_target(target: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("{}: {error}", target.display())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{}: target must be a real folder",
            target.display()
        ));
    }
    let mut entries =
        fs::read_dir(target).map_err(|error| format!("{}: {error}", target.display()))?;
    if let Some(entry) = entries.next() {
        entry.map_err(|error| format!("{}: {error}", target.display()))?;
        return Err(format!("{}: target folder is not empty", target.display()));
    }
    Ok(true)
}

fn create_folder(path: &Path) -> Result<(), String> {
    fs::create_dir(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn remove_folder(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

fn collect_error(result: Result<(), String>, errors: &mut Vec<String>) {
    if let Err(error) = result {
        errors.push(error);
    }
}
