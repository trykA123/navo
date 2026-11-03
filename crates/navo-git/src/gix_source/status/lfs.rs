use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use gix::attrs::StateRef;
use gix::bstr::ByteSlice;
use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};
use gix::status::{Item, index_worktree};
use sha2::{Digest, Sha256};

use crate::GitError;

/// Largest LFS file hashed here; a bigger one hands the repository to the Git command line.
pub(super) const SIZE_CAP: u64 = 512 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;
const NAME: &str = "lfs";

/// Removes the lfs driver from the in-memory config, so gix compares the raw file with the
/// index entry and never starts `git-lfs`. Only used once the driver is known to be the standard one.
pub(super) fn drop_driver(repo: &mut gix::Repository) -> Result<(), GitError> {
    let mut config = repo.config_snapshot_mut();
    while config.remove_section("filter", Some(NAME.into())).is_some() {}
    config
        .commit()
        .map(drop)
        .map_err(|error| super::gix_command("status", error))
}

pub(super) enum Verdict {
    /// Not a stat-dirty LFS file, or its pointer differs from the index entry.
    Modified,
    /// The content produces the committed pointer, so only the stat changed.
    Unchanged,
    /// The stop flag rose while the file was read.
    Stopped,
}

/// Decides whether a path gix reports as modified is only stat-dirty, by building the LFS
/// pointer of the worktree file and comparing its blob id with the index entry's.
pub(super) struct Pointers<'repo> {
    repo: &'repo gix::Repository,
    root: PathBuf,
    stack: gix::AttributeStack<'repo>,
    selected: gix::attrs::search::Outcome,
}

impl<'repo> Pointers<'repo> {
    pub(super) fn new(repo: &'repo gix::Repository, root: &Path) -> Result<Self, GitError> {
        let fail = |error: gix::Error| super::gix_command("status", error);
        let index = repo.index_or_empty().map_err(fail)?;
        let stack = repo
            .attributes_only(
                &index,
                gix::worktree::stack::state::attributes::Source::WorktreeThenIdMapping,
            )
            .map_err(|error| fail(gix::Error::from_error(error)))?;
        let selected = stack.selected_attribute_matches(["filter"]);
        Ok(Self {
            repo,
            root: root.to_path_buf(),
            stack,
            selected,
        })
    }

    pub(super) fn verdict(&mut self, item: &Item, stop: &AtomicBool) -> Result<Verdict, GitError> {
        let Item::IndexWorktree(index_worktree::Item::Modification {
            entry,
            rela_path,
            status:
                EntryStatus::Change(Change::Modification {
                    executable_bit_changed: false,
                    content_change: Some(()),
                    ..
                }),
            ..
        }) = item
        else {
            return Ok(Verdict::Modified);
        };
        if !is_file_mode(entry.mode) || !self.is_lfs(rela_path.as_bstr(), entry.mode) {
            return Ok(Verdict::Modified);
        }
        let Some(path) = self.repo.workdir_path(rela_path) else {
            return Ok(Verdict::Modified);
        };
        let Some(pointer) = self.pointer(&path, stop)? else {
            return Ok(Verdict::Stopped);
        };
        let id = gix::objs::compute_hash(
            self.repo.object_hash(),
            gix::objs::Kind::Blob,
            &pointer.unwrap_or_default(),
        );
        Ok(match id {
            Ok(id) if id == entry.id => Verdict::Unchanged,
            _ => Verdict::Modified,
        })
    }

    fn is_lfs(&mut self, rela_path: &gix::bstr::BStr, mode: gix::index::entry::Mode) -> bool {
        let Ok(platform) = self.stack.at_entry(rela_path, Some(mode)) else {
            return false;
        };
        platform.matching_attributes(&mut self.selected);
        self.selected.iter_selected().any(
            |found| matches!(found.assignment.state, StateRef::Value(value) if value.as_bstr() == NAME),
        )
    }

    /// `None` when stopped; `Some(None)` when the file cannot be read or is no regular file, so
    /// the path stays modified; `Some(Some(bytes))` with the pointer text, empty for an empty file.
    fn pointer(&self, path: &Path, stop: &AtomicBool) -> Result<Option<Option<Vec<u8>>>, GitError> {
        let Ok(mut file) = File::open(path) else {
            return Ok(Some(None));
        };
        match file.metadata() {
            Ok(meta) if meta.is_file() && meta.len() <= SIZE_CAP => {}
            Ok(meta) if meta.is_file() => return Err(self.too_large()),
            _ => return Ok(Some(None)),
        }
        let mut hasher = Sha256::new();
        let mut size = 0_u64;
        let mut chunk = vec![0_u8; CHUNK];
        loop {
            if stop.load(Ordering::Acquire) {
                return Ok(None);
            }
            let Ok(read) = file.read(&mut chunk) else {
                return Ok(Some(None));
            };
            if read == 0 {
                break;
            }
            size += read as u64;
            if size > SIZE_CAP {
                return Err(self.too_large());
            }
            hasher.update(&chunk[..read]);
        }
        Ok(Some(Some(pointer_text(&hasher.finalize(), size))))
    }

    fn too_large(&self) -> GitError {
        GitError::FilterRefused {
            path: self.root.clone(),
            filter: NAME.to_owned(),
        }
    }
}

/// The pointer git-lfs stores for `size` bytes with this SHA-256; git-lfs keeps an empty file empty.
fn pointer_text(digest: &[u8], size: u64) -> Vec<u8> {
    if size == 0 {
        return Vec::new();
    }
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("version https://git-lfs.github.com/spec/v1\noid sha256:{hex}\nsize {size}\n")
        .into_bytes()
}

fn is_file_mode(mode: gix::index::entry::Mode) -> bool {
    mode == gix::index::entry::Mode::FILE || mode == gix::index::entry::Mode::FILE_EXECUTABLE
}
