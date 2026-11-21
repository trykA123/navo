//! Fetches one remote ref, shallow (depth 1), into a bare cache repository: one connection lists
//! the candidate refs, a second fetches the chosen one, both over a capped `Http` backend (NC220).

use std::num::NonZeroU32;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use gix::bstr::BStr;
use gix::progress::Discard;
use gix::protocol::handshake::Ref;
use gix::remote::{Direction, fetch::Shallow, fetch::Tags, ref_map};
use gix_transport::client::blocking_io::http::Http;

pub(crate) mod job;

use self::job::Job;
use crate::lent::lend;
use crate::{Budget, Capped, DeadlineGuard, Limits, RemoteProblem, RemoteRev, RemoteUrl};
use crate::{candidates, choose, problem_of};

/// Fetches `rev` of `url` into the bare repository at `repo_dir` (created when missing or empty)
/// through `http`, within `limits`. Returns the local ref name; a panic inside is `Broken`.
#[expect(
    clippy::too_many_arguments,
    reason = "the signature is fixed by plan row RG5b; RG6a calls it as is"
)]
pub fn fetch_with<H: Http>(
    repo_dir: &Path,
    url: &RemoteUrl,
    rev: &RemoteRev,
    http: H,
    limits: Limits,
) -> Result<String, RemoteProblem> {
    let run = AssertUnwindSafe(|| run(&Job::open(repo_dir, url, rev)?, http, limits));
    catch_unwind(run).unwrap_or(Err(RemoteProblem::Broken))
}

/// Runs both connections of `job` under one byte budget and deadline timer; maps the failure.
fn run<H: Http>(job: &Job<'_>, http: H, limits: Limits) -> Result<String, RemoteProblem> {
    let budget = Arc::new(Budget::new(limits));
    let interrupt = Arc::new(AtomicBool::new(false));
    let guard = DeadlineGuard::start(Arc::clone(&budget), Arc::clone(&interrupt))
        .map_err(|_| RemoteProblem::Broken)?;
    let mut capped = Capped::new(http, Arc::clone(&budget));
    let result = fetch_ref(job, &mut capped, &interrupt);
    // The join inside the drop makes the timer's `TimedOut` visible to `problem_of`.
    drop(guard);
    let fetched = result.map_err(|err| problem_of(&err, &budget))?;
    fetched.map(|()| job.local.clone())
}

/// Connection 1: the refs the server lists for the candidates of the rev (ls-refs prefixes).
fn list<C: Http>(remote: &gix::Remote<'_>, job: &Job<'_>, http: &mut C) -> gix::Result<Vec<Ref>> {
    let mut extra = Vec::new();
    for name in candidates(job.rev) {
        let spec = gix::refspec::parse(BStr::new(&name), gix::refspec::parse::Operation::Fetch)?;
        extra.push(spec.to_owned());
    }
    let options = ref_map::Options {
        prefix_from_spec_as_filter_on_remote: true,
        extra_refspecs: extra,
        ..Default::default()
    };
    let (map, _) = remote
        .to_connection_with_transport(lend(http, &job.url))
        .with_credentials(|_| Ok(None))
        .ref_map(Discard, options)?;
    Ok(map.remote_refs)
}

/// The object id the ref `name` points to in `refs`, if listed and born.
fn listed(refs: &[Ref], name: &str) -> Option<gix::ObjectId> {
    let found = refs.iter().map(Ref::unpack).find(|(n, _, _)| *n == name);
    found.and_then(|(_, id, _)| id.map(ToOwned::to_owned))
}

/// Lists, chooses, then fetches the chosen ref at depth 1 to the job's local ref (connection 2).
/// The outer error is gix's; the inner one a problem found here (no such rev, ref moved).
fn fetch_ref<C: Http>(
    job: &Job<'_>,
    http: &mut C,
    interrupt: &AtomicBool,
) -> gix::Result<Result<(), RemoteProblem>> {
    let remote = job
        .repo
        .remote_at_without_url_rewrite(job.url.clone())?
        .with_fetch_tags(Tags::None);
    let refs = list(&remote, job, http)?;
    let names: Vec<String> = refs.iter().map(|r| r.unpack().0.to_string()).collect();
    let full = match choose(&names, job.rev) {
        Ok(full) => full,
        Err(problem) => return Ok(Err(problem)),
    };
    let want = listed(&refs, &full);
    let spec = format!("+{full}:{}", job.local);
    let remote = remote.with_refspecs([spec.as_str()], Direction::Fetch)?;
    let prepare = remote
        .to_connection_with_transport(lend(http, &job.url))
        .with_credentials(|_| Ok(None))
        .prepare_fetch(Discard, ref_map::Options::default())?;
    // The ref may move or vanish between the connections; fetch only what connection 1 listed.
    if want.is_none() || listed(&prepare.ref_map().remote_refs, &full) != want {
        return Ok(Err(RemoteProblem::NoSuchRevision));
    }
    prepare
        .with_shallow(Shallow::DepthAtRemote(NonZeroU32::MIN))
        .receive(Discard, interrupt)?;
    let reference = job.repo.try_find_reference(job.local.as_str())?;
    let now = reference.and_then(|r| r.target().try_id().map(ToOwned::to_owned));
    Ok((now == want)
        .then_some(())
        .ok_or(RemoteProblem::NoSuchRevision))
}
