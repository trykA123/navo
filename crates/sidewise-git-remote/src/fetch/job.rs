//! The cache repository side of a fetch: isolated open options, the local ref name, and the
//! `Job` that holds the open repository, the parsed address and the rev.

use std::path::Path;

use gix::bstr::BStr;

use crate::{RemoteProblem, RemoteRev, RemoteUrl};

/// The open options of a cache repository: no global, system or environment config (NC219),
/// and the per-object and thread caps of NC221.
pub(crate) fn open_options() -> gix::open::Options {
    gix::open::Options::isolated()
        .cli_overrides(["gitoxide.objects.allocLimit=67108864", "pack.threads=4"])
}

/// The local ref a revision is fetched to: `refs/sidewise/<16 hex of blake3(rev)>`.
pub(crate) fn local_ref(rev: &RemoteRev) -> String {
    let hash = blake3::hash(rev.as_str().as_bytes());
    format!("refs/sidewise/{}", &hash.to_hex()[..16])
}

/// Opens the cache repository, or creates it when `dir` is missing or empty.
fn open_or_init(dir: &Path) -> gix::Result<gix::Repository> {
    let empty = std::fs::read_dir(dir).map_or(true, |mut entries| entries.next().is_none());
    if empty {
        let kind = gix::create::Kind::Bare;
        let options = gix::create::Options::default();
        let repo = gix::ThreadSafeRepository::init_opts(dir, kind, options, open_options())?;
        return Ok(repo.to_thread_local());
    }
    gix::open_opts(dir, open_options())
}

/// One fetch: the open cache repository, the remote address, the rev and its local ref.
pub(super) struct Job<'a> {
    pub(super) repo: gix::Repository,
    pub(super) url: gix::Url,
    pub(super) rev: &'a RemoteRev,
    pub(super) local: String,
}

impl<'a> Job<'a> {
    /// Checks the rev again (`NoSuchRevision`), then opens the repository (`Broken` on failure).
    pub(super) fn open(
        dir: &Path,
        url: &RemoteUrl,
        rev: &'a RemoteRev,
    ) -> Result<Self, RemoteProblem> {
        if gix::validate::reference::name_partial(BStr::new(rev.as_str())).is_err() {
            return Err(RemoteProblem::NoSuchRevision);
        }
        let repo = open_or_init(dir).map_err(|_| RemoteProblem::Broken)?;
        let url = gix::url::parse(BStr::new(url.as_str())).map_err(|_| RemoteProblem::Broken)?;
        let local = local_ref(rev);
        Ok(Self {
            repo,
            url,
            rev,
            local,
        })
    }
}
