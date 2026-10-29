use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use gix_config::file::Metadata;
use gix_config::{File, Source};
use gix_discover::path as discover;

use crate::GitError;

mod extensions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFormat {
    Sha1,
    Sha256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefStorage {
    Files,
    Reftable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFormat {
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub bare: bool,
    pub version: u32,
    pub object_format: ObjectFormat,
    pub ref_storage: RefStorage,
}

impl RepoFormat {
    pub fn read(repo_path: &Path) -> Result<Self, GitError> {
        let located = locate(repo_path)?;
        if !validate_git_dir(&located.git_dir, &located.common_dir)? {
            return Err(GitError::NotARepository {
                path: repo_path.to_path_buf(),
            });
        }
        let config_path = located.common_dir.join("config");
        let config = read_config(&config_path, Source::Local)?;
        let version = read_version(&config, &config_path)?;
        let extensions = extensions::read(&config, version, &config_path)?;
        let common_bare = read_bare(&config, &config_path)?;
        let worktree = if extensions.worktree_config {
            read_worktree_config(&located.git_dir)?
        } else {
            None
        };
        let worktree_bare = match &worktree {
            Some(worktree) => read_bare(worktree, &located.git_dir.join("config.worktree"))?,
            None => None,
        };
        let bare = bare_for(&located, common_bare, worktree_bare);
        Ok(Self {
            git_dir: located.git_dir,
            common_dir: located.common_dir,
            bare,
            version,
            object_format: extensions.object_format,
            ref_storage: extensions.ref_storage,
        })
    }
}

struct Located {
    git_dir: PathBuf,
    common_dir: PathBuf,
    linked_worktree: bool,
}

fn locate(repo: &Path) -> Result<Located, GitError> {
    let dot_git = repo.join(".git");
    match fs::metadata(&dot_git) {
        Ok(metadata) if metadata.is_file() => {
            let git_dir = discover::from_gitdir_file(&dot_git).map_err(|error| {
                repository_file_error(
                    &dot_git,
                    error.downcast_any_ref::<io::Error>().map(io::Error::kind),
                    error.to_string(),
                )
            })?;
            let common = common_dir(&git_dir)?;
            let common_dir = common.clone().unwrap_or_else(|| git_dir.clone());
            Ok(Located {
                git_dir,
                common_dir,
                linked_worktree: common.is_some(),
            })
        }
        Ok(metadata) if metadata.is_dir() => {
            let common = common_dir(&dot_git)?;
            let common_dir = common.clone().unwrap_or_else(|| dot_git.clone());
            Ok(Located {
                git_dir: dot_git,
                common_dir,
                linked_worktree: common.is_some(),
            })
        }
        Ok(_) => Err(GitError::InvalidRepositoryFile {
            path: dot_git,
            detail: "neither a file nor a directory".to_owned(),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if is_file(&repo.join("config"))? && is_file(&repo.join("HEAD"))? {
                Ok(Located {
                    git_dir: repo.to_path_buf(),
                    common_dir: repo.to_path_buf(),
                    linked_worktree: false,
                })
            } else {
                Err(GitError::NotARepository {
                    path: repo.to_path_buf(),
                })
            }
        }
        Err(error) => Err(GitError::Io {
            path: dot_git,
            kind: error.kind(),
        }),
    }
}

fn common_dir(git_dir: &Path) -> Result<Option<PathBuf>, GitError> {
    let path = git_dir.join("commondir");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(GitError::Io {
                path,
                kind: error.kind(),
            });
        }
    };
    let value = strip_line_ending(&bytes);
    if value.is_empty() {
        return Err(GitError::InvalidRepositoryFile {
            path,
            detail: "the commondir file is empty".to_owned(),
        });
    }
    let target = path_from_bytes(&path, value)?;
    Ok(Some(if target.is_absolute() {
        target
    } else {
        git_dir.join(target)
    }))
}

fn strip_line_ending(bytes: &[u8]) -> &[u8] {
    if let Some(value) = bytes.strip_suffix(b"\r\n") {
        value
    } else if let Some(value) = bytes.strip_suffix(b"\n") {
        value
    } else {
        bytes
    }
}

#[cfg(unix)]
fn path_from_bytes(_source: &Path, bytes: &[u8]) -> Result<PathBuf, GitError> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(windows)]
fn path_from_bytes(source: &Path, bytes: &[u8]) -> Result<PathBuf, GitError> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| GitError::InvalidRepositoryFile {
        path: source.to_path_buf(),
        detail: "the commondir path is not valid UTF-8".to_owned(),
    })?;
    Ok(PathBuf::from(text))
}

fn validate_git_dir(git_dir: &Path, common_dir: &Path) -> Result<bool, GitError> {
    let head = is_file(&git_dir.join("HEAD"))?;
    let objects = is_dir(&common_dir.join("objects"))?;
    let refs = is_dir(&common_dir.join("refs"))?;
    Ok(head && objects && refs)
}

fn is_file(path: &Path) -> Result<bool, GitError> {
    metadata_kind(path, fs::Metadata::is_file)
}

fn is_dir(path: &Path) -> Result<bool, GitError> {
    metadata_kind(path, fs::Metadata::is_dir)
}

fn metadata_kind(path: &Path, check: impl FnOnce(&fs::Metadata) -> bool) -> Result<bool, GitError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(check(&metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(GitError::Io {
            path: path.to_path_buf(),
            kind: error.kind(),
        }),
    }
}

fn read_config(path: &Path, source: Source) -> Result<File, GitError> {
    let bytes = fs::read(path).map_err(|error| GitError::Io {
        path: path.to_path_buf(),
        kind: error.kind(),
    })?;
    let metadata = Metadata::from(source).at(path.to_path_buf());
    File::from_bytes_no_includes(&bytes, metadata, Default::default())
        .map_err(|error| config_error(path, None, error.to_string()))
}

fn read_worktree_config(git_dir: &Path) -> Result<Option<File>, GitError> {
    let path = git_dir.join("config.worktree");
    match read_config(&path, Source::Worktree) {
        Ok(config) => Ok(Some(config)),
        Err(GitError::Io {
            kind: io::ErrorKind::NotFound,
            ..
        }) => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_bare(config: &File, path: &Path) -> Result<Option<bool>, GitError> {
    config.boolean_by("core", None, "bare").map_err(|error| {
        config_error(
            path,
            error.downcast_any_ref::<io::Error>().map(io::Error::kind),
            error.to_string(),
        )
    })
}

fn bare_for(located: &Located, common: Option<bool>, worktree: Option<bool>) -> bool {
    if located.linked_worktree {
        return worktree.unwrap_or(false);
    }
    worktree
        .or(common)
        .unwrap_or_else(|| !git_dir_is_dot_git(&located.git_dir))
}

fn git_dir_is_dot_git(git_dir: &Path) -> bool {
    git_dir.file_name() == Some(OsStr::new(".git"))
}

fn read_version(config: &File, path: &Path) -> Result<u32, GitError> {
    let version = config
        .integer_by("core", None, "repositoryformatversion")
        .map_err(|error| {
            config_error(
                path,
                error.downcast_any_ref::<io::Error>().map(io::Error::kind),
                error.to_string(),
            )
        })?;
    match version {
        None | Some(0) => Ok(0),
        Some(1) => Ok(1),
        Some(version) => Err(GitError::Unsupported {
            operation: format!("repository format version {version}"),
        }),
    }
}

fn config_error(path: &Path, kind: Option<io::ErrorKind>, detail: String) -> GitError {
    match kind {
        Some(kind) => GitError::Io {
            path: path.to_path_buf(),
            kind,
        },
        None => invalid_config(path, detail),
    }
}

fn invalid_config(path: &Path, detail: impl Into<String>) -> GitError {
    GitError::InvalidConfig {
        path: path.to_path_buf(),
        detail: detail.into(),
    }
}

fn repository_file_error(path: &Path, kind: Option<io::ErrorKind>, detail: String) -> GitError {
    match kind {
        Some(kind) => GitError::Io {
            path: path.to_path_buf(),
            kind,
        },
        None => GitError::InvalidRepositoryFile {
            path: path.to_path_buf(),
            detail,
        },
    }
}

#[cfg(test)]
mod tests;
