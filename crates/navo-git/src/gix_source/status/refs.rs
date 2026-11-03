use std::sync::atomic::{AtomicBool, Ordering};

use gix::bstr::ByteSlice;
use gix::remote::Direction;

use super::super::{gix_command, object_id};
use crate::GitError;
use crate::status::{Head, Upstream};

const CHECK_EVERY: u32 = 256;

/// `HEAD` as the status header shows it, and the commit it names.
pub(super) struct HeadInfo {
    pub head: Head,
    pub id: Option<gix::hash::ObjectId>,
    name: Option<gix::refs::FullName>,
}

pub(super) fn head(repo: &gix::Repository) -> Result<HeadInfo, GitError> {
    let head = repo.head().map_err(|error| gix_command("status", error))?;
    let id = head.id().map(|id| id.detach());
    let name = head.referent_name().map(ToOwned::to_owned);
    let short = name
        .as_ref()
        .map(|name| name.shorten().to_str_lossy().into_owned());
    let head = match (short, id) {
        (Some(short), Some(_)) => Head::Branch(short),
        (Some(short), None) => Head::Unborn(short),
        (None, Some(id)) => Head::Detached(object_id(&id)?),
        (None, None) => Head::Unborn(String::new()),
    };
    Ok(HeadInfo { head, id, name })
}

/// Ahead and behind of `head` against its configured remote-tracking branch, as of the last fetch.
/// A configured branch whose ref is gone reports 0 and 0, as `git status` does.
pub(super) fn upstream(
    repo: &gix::Repository,
    head: &HeadInfo,
    stop: &AtomicBool,
) -> Result<Option<Upstream>, GitError> {
    let (Some(name), Some(tip)) = (&head.name, head.id) else {
        return Ok(None);
    };
    let Some(Ok(tracking)) = repo.branch_remote_tracking_ref_name(name.as_ref(), Direction::Fetch)
    else {
        return Ok(None);
    };
    let label = tracking.shorten().to_str_lossy().into_owned();
    let remote = repo
        .try_find_reference(tracking.as_ref())
        .map_err(|error| gix_command("status", error))?;
    let Some(mut remote) = remote else {
        return Ok(Some(Upstream {
            name: label,
            ahead: 0,
            behind: 0,
        }));
    };
    let base = remote
        .peel_to_id()
        .map_err(|error| gix_command("status", error))?
        .detach();
    Ok(Some(Upstream {
        name: label,
        ahead: count(repo, tip, base, stop)?,
        behind: count(repo, base, tip, stop)?,
    }))
}

/// Commits reachable from `from` and not from `hidden`; stops early when `stop` is set.
fn count(
    repo: &gix::Repository,
    from: gix::hash::ObjectId,
    hidden: gix::hash::ObjectId,
    stop: &AtomicBool,
) -> Result<u32, GitError> {
    let walk = repo
        .rev_walk([from])
        .with_hidden([hidden])
        .all()
        .map_err(|error| gix_command("status", error))?;
    let mut total = 0_u32;
    for commit in walk {
        commit.map_err(|error| gix_command("status", gix::Error::from_error(error)))?;
        total = total.saturating_add(1);
        if total.is_multiple_of(CHECK_EVERY) && stop.load(Ordering::Acquire) {
            break;
        }
    }
    Ok(total)
}
