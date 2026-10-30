use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use gix::bstr::ByteSlice;

use crate::format::{ObjectFormat, RefStorage, RepoFormat};
use crate::{
    BlobReader, Change, GitError, GitPath, GitRef, GitSource, IndexEntry, ObjectId, OtherPath,
    Others, RefKind, Stage, Target, TreeEntry, TreeKind, WorktreeBlob,
};

mod peel;

pub const MAX_BLOB: u64 = 256 * 1024 * 1024;

pub struct GixGit {
    repo: PathBuf,
}

impl GixGit {
    pub fn new(repo: &Path) -> Result<Self, GitError> {
        let format = RepoFormat::read(repo)?;
        if format.object_format != ObjectFormat::Sha1 {
            return Err(GitError::Unsupported {
                operation: "SHA-256 repositories".to_owned(),
            });
        }
        if format.ref_storage != RefStorage::Files {
            return Err(GitError::Unsupported {
                operation: "reftable references".to_owned(),
            });
        }
        open_repo(repo)?;
        Ok(Self {
            repo: repo.to_path_buf(),
        })
    }

    fn open(&self) -> Result<gix::Repository, GitError> {
        open_repo(&self.repo)
    }

    fn commit(&self, name: &str) -> Result<Option<gix::hash::ObjectId>, GitError> {
        let repo = self.open()?;
        let specification = format!("{name}^{{commit}}");
        let id = match catch_unwind(AssertUnwindSafe(|| repo.rev_parse(specification.as_str()))) {
            Ok(Ok(specification)) => match specification.single() {
                Some(id) => id.detach(),
                None => return Ok(None),
            },
            Ok(Err(error)) if missing_revision(&error) => return Ok(None),
            Ok(Err(error)) => return Err(gix_command("rev-parse", error)),
            Err(_) => return Err(panicked("rev-parse")),
        };
        peel::valid_commit(&repo, id)
    }
}

impl GitSource for GixGit {
    fn resolve(&self, name: &str) -> Result<Option<ObjectId>, GitError> {
        self.commit(name)?.map(|id| object_id(&id)).transpose()
    }

    fn refs(&self) -> Result<Vec<GitRef>, GitError> {
        let repo = self.open()?;
        let mut refs = Vec::new();
        let references = repo
            .references()
            .map_err(|error| gix_command("for-each-ref", error))?;
        let iterator = references
            .all()
            .map_err(|error| gix_command("for-each-ref", error))?;
        for reference in iterator {
            let mut reference = reference.map_err(|error| gix_command("for-each-ref", error))?;
            let name = reference.name().as_bstr().as_bytes().to_vec();
            let target = match reference.try_id() {
                Some(id) => id.detach(),
                None => match reference.follow_to_object() {
                    Ok(id) => id.detach(),
                    Err(error) if missing_reference(&error) => continue,
                    Err(error) => return Err(gix_command("for-each-ref", error)),
                },
            };
            let name = String::from_utf8(name).map_err(|_| protocol())?;
            refs.push(GitRef {
                kind: ref_kind(&name),
                name,
                target: object_id(&target)?,
            });
        }
        refs.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(refs)
    }

    fn list_tree(&self, rev: &str) -> Result<Vec<TreeEntry>, GitError> {
        let repo = self.open()?;
        let id = match catch_unwind(AssertUnwindSafe(|| repo.rev_parse_single(rev))) {
            Ok(Ok(id)) => id.detach(),
            Ok(Err(error)) => return Err(gix_command("ls-tree", error)),
            Err(_) => return Err(panicked("ls-tree")),
        };
        let tree = peel::tree(&repo, id)?;
        let mut recorder = gix::traverse::tree::Recorder::default();
        tree.traverse()
            .depthfirst(&mut recorder)
            .map_err(|error| gix_command("ls-tree", error))?;
        let mut entries = Vec::with_capacity(recorder.records.len());
        for record in recorder.records {
            if record.mode.is_tree() {
                continue;
            }
            let kind = tree_kind(record.mode);
            let size = match kind {
                TreeKind::Submodule => None,
                TreeKind::File | TreeKind::Link | TreeKind::Folder => {
                    let header = repo
                        .find_header(record.oid)
                        .map_err(|error| gix_command("ls-tree", error))?;
                    Some(header.size())
                }
            };
            entries.push(TreeEntry {
                path: GitPath::new(record.filepath.as_bytes().to_vec()),
                id: object_id(&record.oid)?,
                kind,
                size,
            });
        }
        Ok(entries)
    }

    fn read_blob(&self, id: &ObjectId) -> Result<BlobReader, GitError> {
        let repo = self.open()?;
        let gix_id = gix::hash::ObjectId::try_from(id.as_bytes()).map_err(|_| protocol())?;
        let header = repo
            .try_find_header(gix_id)
            .map_err(|error| gix_command("cat-file", error))?
            .ok_or(GitError::MissingObject { id: *id })?;
        if header.kind() != gix::objs::Kind::Blob {
            return Err(GitError::Unsupported {
                operation: "reading a non-blob object as a blob".to_owned(),
            });
        }
        if header.size() > MAX_BLOB {
            return Err(GitError::Unsupported {
                operation: "reading a blob over 256 MiB through gix".to_owned(),
            });
        }
        let object = repo
            .try_find_object(gix_id)
            .map_err(|error| gix_command("cat-file", error))?
            .ok_or(GitError::MissingObject { id: *id })?;
        if object.kind != gix::objs::Kind::Blob {
            return Err(GitError::Unsupported {
                operation: "reading a non-blob object as a blob".to_owned(),
            });
        }
        Ok(Box::new(Cursor::new(object.detach().data)))
    }

    fn read_blob_worktree_form(
        &self,
        _rev: &str,
        _path: &GitPath,
    ) -> Result<WorktreeBlob, GitError> {
        Err(unsupported("read_blob_worktree_form"))
    }

    fn changes(&self, _a: &str, _target: Target<'_>) -> Result<Vec<Change>, GitError> {
        Err(unsupported("changes"))
    }

    fn merge_base(&self, a: &str, b: &str) -> Result<Option<ObjectId>, GitError> {
        let Some(left) = self.commit(a)? else {
            return Err(unknown_revision("merge-base", a, b));
        };
        let Some(right) = self.commit(b)? else {
            return Err(unknown_revision("merge-base", a, b));
        };
        let repo = self.open()?;
        let base = repo
            .merge_base(left, right)
            .map_err(|error| gix_command("merge-base", error))?;
        base.map(|id| object_id(&id.detach())).transpose()
    }

    fn index_entries(&self, _stage: Stage) -> Result<Vec<IndexEntry>, GitError> {
        Err(unsupported("index_entries"))
    }

    fn others(&self, _which: Others) -> Result<Vec<OtherPath>, GitError> {
        Err(unsupported("others"))
    }
}

fn open_repo(path: &Path) -> Result<gix::Repository, GitError> {
    let options = gix::open::Options::default()
        .bail_if_untrusted(true)
        .config_overrides([format!("gitoxide.objects.allocLimit={MAX_BLOB}")]);
    Ok(options
        .open(path)
        .map_err(|error| open_error(error, path))?
        .to_thread_local())
}

fn object_id(id: &gix::hash::ObjectId) -> Result<ObjectId, GitError> {
    ObjectId::from_bytes(id.as_slice()).ok_or_else(protocol)
}

fn tree_kind(mode: gix::objs::tree::EntryMode) -> TreeKind {
    if mode.is_commit() {
        TreeKind::Submodule
    } else if mode.is_link() {
        TreeKind::Link
    } else {
        TreeKind::File
    }
}

fn ref_kind(name: &str) -> RefKind {
    if name.starts_with("refs/heads/") {
        RefKind::Branch
    } else if name.starts_with("refs/tags/") {
        RefKind::Tag
    } else if name.starts_with("refs/remotes/") {
        RefKind::Remote
    } else if name == "HEAD" {
        RefKind::Head
    } else {
        RefKind::Other
    }
}

fn unsupported(operation: &str) -> GitError {
    GitError::Unsupported {
        operation: operation.to_owned(),
    }
}

fn protocol() -> GitError {
    GitError::Protocol {
        detail: "unexpected Git data from gix".to_owned(),
    }
}

fn unknown_revision(operation: &str, a: &str, b: &str) -> GitError {
    GitError::Command {
        args: format!("gix {operation} {a} {b}"),
        status: Some(128),
        stderr: "unknown revision".to_owned(),
    }
}

fn gix_command(operation: &str, error: gix::Error) -> GitError {
    GitError::Command {
        args: format!("gix {operation}"),
        status: None,
        stderr: error.to_string(),
    }
}

fn panicked(operation: &str) -> GitError {
    GitError::Protocol {
        detail: format!("gix {operation} panicked on malformed object data"),
    }
}

fn open_error(error: gix::Error, repo: &Path) -> GitError {
    if let Some(io) = error.downcast_any_ref::<std::io::Error>() {
        return match io.kind() {
            std::io::ErrorKind::NotFound => GitError::NotARepository {
                path: repo.to_path_buf(),
            },
            kind => GitError::Io {
                path: repo.to_path_buf(),
                kind,
            },
        };
    }
    let text = error.to_string();
    let lower = text.to_lowercase();
    if lower.contains("considered unsafe") || lower.contains("not owned by the current user") {
        GitError::UnsafeDirectory {
            path: repo.to_path_buf(),
        }
    } else if lower.contains("not a git repository") || lower.contains("not a repository") {
        GitError::NotARepository {
            path: repo.to_path_buf(),
        }
    } else {
        gix_command("open", error)
    }
}

fn missing_revision(error: &gix::Error) -> bool {
    if error.downcast_any_ref::<std::io::Error>().is_some() {
        return false;
    }
    match error.dominant_class() {
        Some(gix::error::Class::NotFound) => return true,
        Some(gix::error::Class::Validation) | None => {}
        Some(_) => return false,
    }
    if let Some(parse) = error.downcast_any_ref::<gix::revision::spec::parse::Error>() {
        return matches!(
            parse,
            gix::revision::spec::parse::Error::MissingReference { .. }
                | gix::revision::spec::parse::Error::AmbiguousPrefix { .. }
                | gix::revision::spec::parse::Error::AmbiguousRefAndObject { .. }
        );
    }
    let text = error_text(error);
    text.contains("needed a single revision")
        || text.contains("unknown revision")
        || text.contains("ambiguous argument")
        || text.contains("did not resolve to a single object")
        || text.contains("while trying to peel to")
        || text.contains("does not have a reference log")
        || text.contains("object could not be found")
        || (text.contains("ancestor number") && text.contains("out of range"))
}

fn error_text(error: &gix::Error) -> String {
    let mut text = error.to_string();
    for cause in error.iter_errors() {
        text.push('\n');
        text.push_str(&cause.to_string());
    }
    text.to_lowercase()
}

fn missing_reference(error: &gix::Error) -> bool {
    matches!(error.dominant_class(), Some(gix::error::Class::NotFound))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_denied_is_an_io_error_not_unsafe_directory() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let error = gix::Error::from_error(io);
        match open_error(error, Path::new("/repo")) {
            GitError::Io { kind, .. } => assert_eq!(kind, std::io::ErrorKind::PermissionDenied),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn a_trust_refusal_is_an_unsafe_directory() {
        #[derive(Debug)]
        struct Refusal;

        impl std::fmt::Display for Refusal {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(
                    "The git directory is considered unsafe as it's not owned by the current user.",
                )
            }
        }

        impl std::error::Error for Refusal {}

        let error = gix::Error::from_error(Refusal);
        assert!(matches!(
            open_error(error, Path::new("/repo")),
            GitError::UnsafeDirectory { .. }
        ));
    }
}
