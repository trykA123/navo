use gix::hash::ObjectId;
use gix::objs::Kind;

use crate::GitError;

use super::gix_command;

pub(super) fn valid_commit(
    repo: &gix::Repository,
    id: ObjectId,
) -> Result<Option<ObjectId>, GitError> {
    let object = repo
        .find_object(id)
        .map_err(|error| gix_command("rev-parse", error))?;
    let Ok(commit) = object.try_into_commit() else {
        return Ok(None);
    };
    match commit.decode() {
        Ok(_) => Ok(Some(commit.id)),
        Err(_) => Ok(None),
    }
}

pub(super) fn tree<'repo>(
    repo: &'repo gix::Repository,
    id: ObjectId,
) -> Result<gix::Tree<'repo>, GitError> {
    let mut id = id;
    loop {
        let object = repo
            .find_object(id)
            .map_err(|error| gix_command("ls-tree", error))?;
        match object.kind {
            Kind::Tree => {
                let tree = object
                    .try_into_tree()
                    .map_err(|error| gix_command("ls-tree", error))?;
                tree.decode()
                    .map_err(|error| gix_command("ls-tree", error))?;
                return Ok(tree);
            }
            Kind::Commit => {
                let commit = object
                    .try_into_commit()
                    .map_err(|error| gix_command("ls-tree", error))?;
                commit
                    .decode()
                    .map_err(|error| gix_command("ls-tree", error))?;
                id = commit
                    .tree_id()
                    .map_err(|error| gix_command("ls-tree", error))?
                    .detach();
            }
            Kind::Tag => {
                let tag = object
                    .try_into_tag()
                    .map_err(|error| gix_command("ls-tree", error))?;
                id = tag
                    .target_id()
                    .map_err(|error| gix_command("ls-tree", error))?
                    .detach();
            }
            Kind::Blob => return Err(not_a_tree()),
        }
    }
}

fn not_a_tree() -> GitError {
    GitError::Command {
        args: "gix ls-tree".to_owned(),
        status: Some(128),
        stderr: "not a tree object".to_owned(),
    }
}
