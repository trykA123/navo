//! Test repositories built in a temporary folder with gix itself, so tests never need `git`.
//! Every commit has a fixed author and time, so object ids are the same on every run.

use std::path::Path;

use gix::bstr::BStr;
use gix::objs::tree::EntryKind;
use gix::refs::transaction::PreviousValue;

/// The fixed author and committer of every fixture commit and tag.
fn signature() -> gix::actor::SignatureRef<'static> {
    gix::actor::SignatureRef {
        name: BStr::new(b"admin"),
        email: BStr::new(b"admin@example.invalid"),
        time: "0 +0000",
    }
}

/// A repository with a work tree in its own temporary folder, removed on drop.
pub struct Fixture {
    /// The temporary folder that holds the work tree and its `.git` folder.
    pub dir: tempfile::TempDir,
}

#[expect(
    clippy::expect_used,
    reason = "a test fixture that cannot be built must stop the test"
)]
impl Fixture {
    /// Creates an empty repository whose `HEAD` names `refs/heads/main`. Panics when it cannot.
    #[must_use]
    pub fn new() -> Fixture {
        let dir = tempfile::tempdir().expect("temporary folder");
        gix::ThreadSafeRepository::init_opts(
            dir.path(),
            gix::create::Kind::WithWorktree,
            gix::create::Options::default(),
            gix::open::Options::isolated(),
        )
        .expect("init repository");
        Fixture { dir }
    }

    /// The work tree folder of the repository.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Commits `files` (path, bytes) as the whole tree on `refs/heads/<branch>`, with the
    /// branch tip as parent if it exists. Returns the new commit id. Panics on a write error.
    #[must_use]
    pub fn commit(&self, branch: &str, files: &[(&str, &[u8])]) -> gix::ObjectId {
        let repo = self.open();
        let mut editor = repo
            .edit_tree(gix::ObjectId::empty_tree(gix::hash::Kind::Sha1))
            .expect("tree editor");
        for (path, bytes) in files {
            let blob = repo.write_blob(bytes).expect("write blob").detach();
            editor
                .upsert(*path, EntryKind::Blob, blob)
                .expect("add tree entry");
        }
        let tree = editor.write().expect("write tree").detach();
        let name = format!("refs/heads/{branch}");
        let parent = repo
            .try_find_reference(name.as_str())
            .expect("look up branch")
            .map(|mut tip| tip.peel_to_id().expect("branch tip").detach());
        repo.commit_as(
            signature(),
            signature(),
            name.as_str(),
            "fixture",
            tree,
            parent,
        )
        .expect("write commit")
        .detach()
    }

    /// Adds an annotated tag `name` that points at the commit `target`. Panics on a write error.
    pub fn tag(&self, name: &str, target: gix::ObjectId) {
        let repo = self.open();
        repo.tag(
            name,
            target,
            gix::objs::Kind::Commit,
            Some(signature()),
            "fixture tag",
            PreviousValue::MustNotExist,
        )
        .expect("write tag");
    }

    /// Opens the fixture repository with only its own config.
    fn open(&self) -> gix::Repository {
        gix::open_opts(self.path(), gix::open::Options::isolated()).expect("open fixture")
    }
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}
