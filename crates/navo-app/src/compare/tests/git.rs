//! Git compare integration tests, including raw reads from partial clones.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc;

use navo_compare::model::{
    CancelToken, EntryKind, HashCache, ObjectId, ReadHint, Reason, RelPath, SideSource, Status,
};
use navo_core::ProviderPath;
use navo_git::{EngineKind, EngineSetting};
use navo_store::CacheDb;
use std::ops::ControlFlow;

use super::handle::{final_status, finish};
use super::{location, write};
use crate::compare::git_side::GitSide;
use crate::compare::handle::{CompareHandle, CompareStart, CompareTarget};
use crate::compare::hash_cache::StoreHashCache;
use crate::compare::settings::{CompareSettings, engine_for};

fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(repo)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// A repository with one commit, or None when no usable git is installed.
fn repo() -> Option<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    git(&path, &["init", "-q"])?;
    write(&path, "a.txt", b"alpha");
    write(&path, "docs/b.txt", b"beta beta");
    git(&path, &["add", "."])?;
    git(&path, &["commit", "-q", "-m", "first"])?;
    Some((dir, path))
}

fn rel(text: &str) -> RelPath {
    RelPath::new(ProviderPath::from_text(text))
}

fn hex(id: &ObjectId) -> String {
    id.as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn a_blob_lists_opens_and_carries_its_object_id() {
    let Some((_dir, path)) = repo() else {
        return;
    };
    let side = GitSide::new(path.clone(), "HEAD".to_owned(), EngineKind::Cli).unwrap();
    let mut found = Vec::new();
    side.list(
        &rel(""),
        &mut |entry| {
            found.push(entry);
            ControlFlow::Continue(())
        },
        &CancelToken::new(),
    )
    .unwrap();
    found.sort_by_key(|entry| entry.rel.as_path().to_text());
    let kinds: Vec<_> = found
        .iter()
        .map(|e| (e.rel.as_path().to_text(), e.kind, e.size))
        .collect();
    assert_eq!(
        kinds,
        [
            ("a.txt".to_owned(), EntryKind::File, 5),
            ("docs".to_owned(), EntryKind::Folder, 0)
        ]
    );
    let mut text = String::new();
    side.open(&rel("a.txt"), ReadHint::Buffered)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "alpha");
    let id = side.object_id(&rel("a.txt")).unwrap();
    assert_eq!(Some(hex(&id)), git(&path, &["rev-parse", "HEAD:a.txt"]));
    assert!(side.read_only());
}

#[test]
fn an_unknown_revision_is_not_a_side() {
    let Some((_dir, path)) = repo() else {
        return;
    };
    assert!(GitSide::new(path, "no-such-rev".to_owned(), EngineKind::Cli).is_err());
}

#[test]
fn a_worktree_compares_equal_to_its_own_revision() {
    let Some((_dir, path)) = repo() else {
        return;
    };
    let (sender, receiver) = mpsc::channel();
    let cache: Arc<dyn HashCache> =
        Arc::new(StoreHashCache::new(CacheDb::open_in_memory().unwrap()));
    let request = CompareStart {
        left: CompareTarget::Local(location(&path)),
        right: CompareTarget::Git {
            repo: path.clone(),
            rev: "HEAD".to_owned(),
        },
        settings: CompareSettings::default(),
        cache,
    };
    let _handle = CompareHandle::start(request, sender);
    let (rows, totals, _) = finish(&receiver);
    assert_eq!(final_status(&rows, "docs/b.txt"), Status::Same);
    assert_eq!((totals.rows, totals.same), (3, 3));
}

#[test]
fn refresh_resolves_the_revision_again() {
    let Some((_dir, path)) = repo() else {
        return;
    };
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "a.txt", b"alpha");
    write(folder.path(), "docs/b.txt", b"beta beta");
    let (sender, receiver) = mpsc::channel();
    let request = CompareStart {
        left: CompareTarget::Local(location(folder.path())),
        right: CompareTarget::Git {
            repo: path.clone(),
            rev: "HEAD".to_owned(),
        },
        settings: CompareSettings::default(),
        cache: Arc::new(StoreHashCache::new(CacheDb::open_in_memory().unwrap())),
    };
    let handle = CompareHandle::start(request, sender);
    finish(&receiver);
    write(&path, "new.txt", b"new");
    assert!(git(&path, &["add", "."]).is_some());
    assert!(git(&path, &["commit", "-q", "-m", "second"]).is_some());
    handle.refresh();
    let (rows, _, _) = finish(&receiver);
    assert_eq!(final_status(&rows, "new.txt"), Status::OnlyRight);
}

/// A SHA-256 repository: the CLI reads it and gix does not, so the outcome shows the engine.
fn sha256_repo() -> Option<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    git(&path, &["init", "-q", "--object-format=sha256"])?;
    write(&path, "a.txt", b"alpha");
    git(&path, &["add", "."])?;
    git(&path, &["commit", "-q", "-m", "first"])?;
    Some((dir, path))
}

#[test]
fn the_chosen_engine_reaches_the_side() {
    let Some((_dir, path)) = sha256_repo() else {
        return;
    };
    let open = |engine| GitSide::new(path.clone(), "HEAD".to_owned(), engine);
    assert!(open(EngineKind::Cli).is_ok());
    assert!(open(EngineKind::Gix).is_err());
}

#[test]
fn gix_reads_a_sha1_repository_as_a_side() {
    let Some((_dir, path)) = repo() else {
        return;
    };
    let side = GitSide::new(path, "HEAD".to_owned(), EngineKind::Gix).unwrap();
    let mut text = String::new();
    side.open(&rel("a.txt"), ReadHint::Buffered)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "alpha");
}

#[test]
fn engine_for_follows_the_setting_and_the_repository_format() {
    let (Some((_sha1, old)), Some((_sha256, new))) = (repo(), sha256_repo()) else {
        return;
    };
    let with = |git_engine| CompareSettings {
        git_engine,
        ..CompareSettings::default()
    };
    assert_eq!(engine_for(&with(EngineSetting::Git), &old), EngineKind::Cli);
    assert_eq!(
        engine_for(&with(EngineSetting::Auto), &old),
        EngineKind::Gix
    );
    assert_eq!(engine_for(&with(EngineSetting::Gix), &old), EngineKind::Gix);
    assert_eq!(
        engine_for(&with(EngineSetting::Auto), &new),
        EngineKind::Cli
    );
    assert_eq!(engine_for(&with(EngineSetting::Gix), &new), EngineKind::Cli);
}

/// Creates a partial clone with two blobs hydrated; returns None when Git cannot initialize it.
fn promisor_repo() -> Option<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "--bare", "r.git"])?;
    let work = root.join("w");
    git(root, &["init", "-q", "w"]).unwrap();
    write(&work, "present.txt", b"raw-bytes\n");
    write(&work, "missing.txt", b"lazy-content\n");
    write(&work, "empty.txt", b"");
    git(&work, &["add", "."]).unwrap();
    git(&work, &["commit", "-q", "-m", "first"]).unwrap();
    git(&work, &["push", "-q", "../r.git", "HEAD:refs/heads/main"]).unwrap();

    let remote = root.join("r.git");
    git(&remote, &["config", "uploadpack.allowFilter", "true"]).unwrap();
    git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]).unwrap();
    let url = format!("file://{}", remote.display());
    git(
        root,
        &["clone", "-q", "--bare", "--filter=blob:none", &url, "p.git"],
    )
    .unwrap();

    let promisor = root.join("p.git");
    write(&promisor, "info/attributes", b"* filter=hostile\n");
    git(
        &promisor,
        &[
            "config",
            "filter.hostile.smudge",
            "nonexistent-hostile-filter",
        ],
    )
    .unwrap();
    git(&promisor, &["config", "filter.hostile.required", "true"]).unwrap();
    git(&promisor, &["cat-file", "blob", "HEAD:present.txt"]).unwrap();
    git(&promisor, &["cat-file", "blob", "HEAD:empty.txt"]).unwrap();
    Some((dir, promisor))
}

#[test]
fn git_raw_odb_skips_filters_and_reports_missing_promisor_blob() {
    let Some((_dir, promisor)) = promisor_repo() else {
        return;
    };
    let local = tempfile::tempdir().unwrap();
    write(local.path(), "present.txt", b"raw-bytes\n");
    write(local.path(), "empty.txt", b"");

    for content_check in [
        navo_store::ContentCheck::Content,
        navo_store::ContentCheck::SizeTime,
    ] {
        for bytes in [b"lazy-content\n".as_slice(), b"".as_slice()] {
            write(local.path(), "missing.txt", bytes);
            let (sender, receiver) = mpsc::channel();
            let options = navo_ui::compare_diff::CompareOptions {
                compare: navo_store::Compare {
                    content_check,
                    ..navo_store::Compare::default()
                },
            };
            let request = CompareStart::new(
                (
                    CompareTarget::Local(location(local.path())),
                    CompareTarget::GitRawOdb {
                        repo: promisor.clone(),
                        rev: "HEAD".to_owned(),
                    },
                ),
                &options,
                Arc::new(StoreHashCache::new(CacheDb::open_in_memory().unwrap())),
            );
            let _handle = CompareHandle::start(request, sender);
            let (rows, _, _) = finish(&receiver);
            if content_check == navo_store::ContentCheck::Content {
                assert_eq!(final_status(&rows, "present.txt"), Status::Same);
            }
            let empty_status = match content_check {
                navo_store::ContentCheck::Content => Status::Same,
                navo_store::ContentCheck::SizeTime => Status::Different {
                    first_difference: None,
                },
            };
            assert_eq!(final_status(&rows, "empty.txt"), empty_status);
            assert_eq!(
                final_status(&rows, "missing.txt"),
                Status::Unavailable(Reason::MissingBlob)
            );
        }
    }
}
