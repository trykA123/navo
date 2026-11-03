use std::time::Duration;

use navo_core::{Location, ProviderId, ProviderPath};
use navo_fs::{CancelToken, JobOutcome, SourceContent};
use navo_shell::SourceKind;

use super::*;
#[cfg(windows)]
use crate::virtual_paste::mark;

fn explained(paste: &StreamPaste) -> Option<String> {
    paste.explain.as_ref().and_then(|explain| explain())
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn nested_folders_are_pasted_with_their_modified_times() {
    let dir = tempfile::tempdir().unwrap();
    let scene = remote(
        vec![
            folder(0, "top"),
            file(1, "top\\inner\\deep.txt", Some(5)),
            file(2, "loose.txt", None),
        ],
        vec![
            Feed::Bytes(Vec::new(), Then::End),
            Feed::Bytes(b"hello".to_vec(), Then::End),
            Feed::Bytes(vec![7; 70_000], Then::End),
        ],
    );
    let outcome = run_into(&scene.paste, dir.path(), &mut |_| {}, &CancelToken::new());
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(
        tree(dir.path()),
        ["loose.txt", "top", "top/inner", "top/inner/deep.txt"]
    );
    let deep = dir.path().join("top/inner/deep.txt");
    assert_eq!(std::fs::read(&deep).unwrap(), b"hello");
    assert_eq!(modified(&deep), time(1));
    let loose = dir.path().join("loose.txt");
    assert_eq!(std::fs::read(&loose).unwrap().len(), 70_000);
    assert_eq!(modified(&loose), time(2));
    assert_eq!(
        *scene.opened.lock().unwrap(),
        [1, 2],
        "folders are never opened"
    );
}

#[test]
fn cancel_while_a_read_blocks_ends_the_job_and_leaves_no_temp() {
    let dir = tempfile::tempdir().unwrap();
    let scene = remote(
        vec![file(0, "big.bin", Some(10_000))],
        vec![Feed::Bytes(vec![1; 4096], Then::Hang)],
    );
    let cancel = CancelToken::new();
    let token = cancel.clone();
    let mut cancel_once_bytes = move |progress: &Progress| {
        if progress.bytes_done > 0 {
            token.cancel();
        }
    };
    let started = std::time::Instant::now();
    let outcome = run_into(&scene.paste, dir.path(), &mut cancel_once_bytes, &cancel);
    assert_eq!(outcome, JobOutcome::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "cancel does not wait for the source"
    );
    assert!(tree(dir.path()).is_empty(), "{:?}", tree(dir.path()));
    assert_eq!(explained(&scene.paste), None, "a cancel is not a failure");
}

#[test]
fn a_source_that_stops_sending_fails_with_the_stall_message() {
    let dir = tempfile::tempdir().unwrap();
    let scene = scene(
        vec![file(0, "slow.bin", Some(10_000))],
        vec![Feed::Bytes(vec![1; 100], Then::Hang)],
        (SourceKind::RemoteSession, Duration::from_millis(300)),
    );
    let outcome = run_into(&scene.paste, dir.path(), &mut |_| {}, &CancelToken::new());
    assert!(matches!(outcome, JobOutcome::Failed { .. }), "{outcome:?}");
    assert!(tree(dir.path()).is_empty());
    assert_eq!(
        explained(&scene.paste).as_deref(),
        Some("The remote session stopped sending data for “slow.bin”.")
    );
}

#[test]
fn a_source_error_mid_stream_fails_the_job_and_removes_the_partial_file() {
    let dir = tempfile::tempdir().unwrap();
    let scene = remote(
        vec![
            file(0, "first.txt", Some(2)),
            file(1, "broken.txt", Some(9_000)),
        ],
        vec![
            Feed::Bytes(b"ok".to_vec(), Then::End),
            Feed::Bytes(vec![3; 5_000], Then::Fail("The connection was reset")),
        ],
    );
    let outcome = run_into(&scene.paste, dir.path(), &mut |_| {}, &CancelToken::new());
    assert!(
        matches!(outcome, JobOutcome::Failed { seq: 1, .. }),
        "{outcome:?}"
    );
    assert_eq!(tree(dir.path()), ["first.txt"]);
    let message = explained(&scene.paste).unwrap();
    assert!(message.contains("“broken.txt”"), "{message}");
    assert!(message.contains("The connection was reset"), "{message}");
}

#[test]
fn an_item_shorter_than_its_known_size_is_not_placed() {
    let dir = tempfile::tempdir().unwrap();
    let scene = remote(
        vec![file(0, "cut.txt", Some(10))],
        vec![Feed::Bytes(b"abc".to_vec(), Then::End)],
    );
    let outcome = run_into(&scene.paste, dir.path(), &mut |_| {}, &CancelToken::new());
    assert!(matches!(outcome, JobOutcome::Failed { .. }), "{outcome:?}");
    assert!(tree(dir.path()).is_empty());
    let message = explained(&scene.paste).unwrap();
    assert!(
        message.contains("ended before all of it arrived"),
        "{message}"
    );
}

#[test]
fn a_mail_saved_as_storage_is_placed_through_the_temp_name_with_its_time() {
    let dir = tempfile::tempdir().unwrap();
    let scene = scene(
        vec![file(0, "Lunch.msg", None)],
        vec![Feed::Storage(b"compound file".to_vec())],
        (SourceKind::Mail, Duration::from_secs(5)),
    );
    let outcome = run_into(&scene.paste, dir.path(), &mut |_| {}, &CancelToken::new());
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(tree(dir.path()), ["Lunch.msg"]);
    let placed = dir.path().join("Lunch.msg");
    assert_eq!(std::fs::read(&placed).unwrap(), b"compound file");
    assert_eq!(modified(&placed), time(0));
}

#[cfg(windows)]
fn zone_of(path: &Path) -> Option<Vec<u8>> {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::read(stream).ok()
}

#[cfg(windows)]
#[test]
fn mail_files_carry_the_mark_of_the_web_and_remote_files_do_not() {
    for (kind, marked) in [(SourceKind::Mail, true), (SourceKind::RemoteSession, false)] {
        let dir = tempfile::tempdir().unwrap();
        let scene = scene(
            vec![file(0, "note.txt", Some(2)), file(1, "mail.msg", None)],
            vec![
                Feed::Bytes(b"hi".to_vec(), Then::End),
                Feed::Storage(b"msg".to_vec()),
            ],
            (kind, Duration::from_secs(5)),
        );
        let outcome = run_into(&scene.paste, dir.path(), &mut |_| {}, &CancelToken::new());
        assert_eq!(outcome, JobOutcome::Done);
        for name in ["note.txt", "mail.msg"] {
            let zone = zone_of(&dir.path().join(name));
            assert_eq!(
                zone.as_deref(),
                marked.then_some(mark::ZONE),
                "{kind:?} {name}"
            );
        }
        let note = dir.path().join("note.txt");
        assert_eq!(
            modified(&note),
            time(0),
            "the mark does not change the time"
        );
    }
}

#[cfg(windows)]
const ZIP_ZONE: &[u8] = b"[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://example.test/\r\n";

/// Pastes two files as an archive clipboard from `zip` and returns the target folder.
#[cfg(windows)]
fn paste_from_archive(zip: std::path::PathBuf) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let scene = scene_in(
        vec![file(0, "note.txt", Some(2)), file(1, "mail.msg", None)],
        vec![
            Feed::Bytes(b"hi".to_vec(), Then::End),
            Feed::Storage(b"msg".to_vec()),
        ],
        (SourceKind::Archive, Duration::from_secs(5)),
        Some(zip),
    );
    let outcome = run_into(&scene.paste, dir.path(), &mut |_| {}, &CancelToken::new());
    assert_eq!(outcome, JobOutcome::Done);
    dir
}

#[cfg(windows)]
fn make_zip(dir: &Path, zone: Option<&[u8]>) -> std::path::PathBuf {
    let zip = dir.join("pack.zip");
    std::fs::write(&zip, b"PK").unwrap();
    if let Some(zone) = zone {
        let mut stream = zip.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        std::fs::write(stream, zone).unwrap();
    }
    zip
}

#[cfg(windows)]
#[test]
fn archive_files_carry_the_zone_of_their_zip_byte_for_byte() {
    let home = tempfile::tempdir().unwrap();
    let dir = paste_from_archive(make_zip(home.path(), Some(ZIP_ZONE)));
    for name in ["note.txt", "mail.msg"] {
        let zone = zone_of(&dir.path().join(name));
        assert_eq!(zone.as_deref(), Some(ZIP_ZONE), "{name}");
    }
}

#[cfg(windows)]
#[test]
fn archive_files_from_an_unmarked_zip_carry_no_mark() {
    let home = tempfile::tempdir().unwrap();
    let dir = paste_from_archive(make_zip(home.path(), None));
    for name in ["note.txt", "mail.msg"] {
        assert_eq!(zone_of(&dir.path().join(name)), None, "{name}");
    }
}

#[cfg(windows)]
#[test]
fn archive_files_from_a_missing_zip_carry_the_internet_mark() {
    let home = tempfile::tempdir().unwrap();
    let dir = paste_from_archive(home.path().join("gone.zip"));
    for name in ["note.txt", "mail.msg"] {
        let zone = zone_of(&dir.path().join(name));
        assert_eq!(zone.as_deref(), Some(mark::ZONE), "{name}");
    }
}

#[test]
fn an_unmarked_stream_never_needs_the_temp_path() {
    let scene = remote(
        vec![file(0, "a.txt", Some(2))],
        vec![Feed::Bytes(b"hi".to_vec(), Then::End)],
    );
    let provider = ProviderId::new("pasted").unwrap();
    let source = scene.paste.sources.find(&provider).unwrap();
    let item = Location::provider_root(provider)
        .join(&ProviderPath::from_text("a.txt"))
        .unwrap();
    let elsewhere = Location::provider_root(ProviderId::new("elsewhere").unwrap());
    let content = source
        .open_into(&item, (&elsewhere, &mut || Ok(())), &CancelToken::new())
        .unwrap();
    let SourceContent::Stream(mut bytes) = content else {
        panic!("expected a stream");
    };
    let mut text = Vec::new();
    std::io::Read::read_to_end(&mut bytes, &mut text).unwrap();
    assert_eq!(text, b"hi");
}
