//! Finds one entry of a commit's tree by path without building the whole index: one tree
//! read per path component. Read-only; refuses the same trees the index refuses on its path.

use std::panic::{AssertUnwindSafe, catch_unwind};

use gix::bstr::ByteSlice;
use gix::objs::tree::{EntryKind as GitKind, EntryMode};
use sidewise_engine::model::Reason;

use crate::index::{Child, MAX_DEPTH, MAX_PATH_BYTES, kind_and_size, object_problem, plain_name};
use crate::repo::Repo;

/// Finds the one entry at `rel` (a `/`-joined UTF-8 path) in `tree` without an index. `None`
/// when absent or when a component before the last is not a folder (that entry is never
/// read). Fails with `Unsupported` for a refused name, a path past a cap or a name two
/// siblings share, `ReadError` for a damaged tree or a parser panic, `MissingBlob` for an
/// absent object.
pub fn lookup(repo: &Repo, tree: gix::ObjectId, rel: &str) -> Result<Option<Child>, Reason> {
    if rel.is_empty() {
        return Ok(None);
    }
    let parts: Vec<&str> = rel.split('/').collect();
    let plain = parts.iter().all(|part| plain_name((*part).into()));
    if !plain || parts.len() > MAX_DEPTH || rel.len() > MAX_PATH_BYTES {
        return Err(Reason::Unsupported);
    }
    let local = repo.local();
    let found = catch_unwind(AssertUnwindSafe(|| walk(&local, tree, &parts)));
    found.map_err(|_| Reason::ReadError)?
}

/// Walks `parts` from the tree `tree`, one folder at a time, and reads the last entry.
fn walk(
    repo: &gix::Repository,
    mut tree: gix::ObjectId,
    parts: &[&str],
) -> Result<Option<Child>, Reason> {
    let Some((last, folders)) = parts.split_last() else {
        return Ok(None);
    };
    for part in folders {
        match find_in(repo, tree, part)? {
            Some((mode, id, _)) if mode.kind() == GitKind::Tree => tree = id,
            _ => return Ok(None),
        }
    }
    let Some((mode, id, name)) = find_in(repo, tree, last)? else {
        return Ok(None);
    };
    let (kind, size) = kind_and_size(repo, mode, &id)?;
    Ok(Some(Child {
        name,
        kind,
        size,
        id,
        readable: true,
    }))
}

/// The mode, id and name of the entry `name` in the tree `tree`, or `None`. Decodes every
/// entry, as the index does: `ReadError` for a damaged entry, `Unsupported` when two
/// entries show as `name` (a lossy non-UTF-8 name can).
fn find_in(
    repo: &gix::Repository,
    tree: gix::ObjectId,
    name: &str,
) -> Result<Option<(EntryMode, gix::ObjectId, String)>, Reason> {
    let tree = repo
        .find_tree(tree)
        .map_err(|error| object_problem(&error))?;
    let mut found = None;
    let mut shown = 0;
    for entry in tree.iter() {
        let entry = entry.map_err(|_| Reason::ReadError)?;
        if entry.filename().to_str_lossy() != name {
            continue;
        }
        shown += 1;
        if shown > 1 {
            return Err(Reason::Unsupported);
        }
        if entry.filename() == name.as_bytes() {
            found = Some((entry.mode(), entry.oid().to_owned(), name.to_owned()));
        }
    }
    Ok(found)
}
