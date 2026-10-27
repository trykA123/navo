use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use rusty_core::{FsError, FsErrorKind};

use super::{Algo, hash_file};
use crate::CancelToken;

const WAIT: Duration = Duration::from_secs(5);

fn run_hash(
    path: PathBuf,
    algo: Algo,
    cancel: CancelToken,
    progress: impl FnMut(u64) + Send + 'static,
) -> Result<String, FsError> {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let result = hash_file(&path, algo, &cancel, progress);
        let _ = send.send(result);
    });
    receive.recv_timeout(WAIT).unwrap()
}

#[test]
fn hash_empty_file_matches_known_vectors() {
    let file = tempfile::NamedTempFile::new().unwrap();
    for (algo, expected) in [
        (Algo::Crc32, "00000000"),
        (Algo::Md5, "d41d8cd98f00b204e9800998ecf8427e"),
        (
            Algo::Sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
    ] {
        let result = run_hash(file.path().into(), algo, CancelToken::new(), |_| {});
        assert_eq!(result.unwrap(), expected);
    }
}

#[test]
fn hash_abc_matches_known_vectors_and_reports_bytes() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"abc").unwrap();
    for (algo, expected) in [
        (Algo::Crc32, "352441c2"),
        (Algo::Md5, "900150983cd24fb0d6963f7d28e17f72"),
        (
            Algo::Sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    ] {
        let progress = Arc::new(Mutex::new(Vec::new()));
        let reported = Arc::clone(&progress);
        let result = run_hash(file.path().into(), algo, CancelToken::new(), move |bytes| {
            reported.lock().unwrap().push(bytes);
        });
        assert_eq!(result.unwrap(), expected);
        assert_eq!(*progress.lock().unwrap(), vec![0, 3]);
    }
}

#[test]
fn hash_cancelled_before_open_returns_cancelled() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = run_hash(file.path().into(), Algo::Md5, cancel, |_| {});
    assert_eq!(result.unwrap_err().kind, FsErrorKind::Cancelled);
}

#[test]
fn hash_cancelled_from_progress_returns_cancelled() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"abc").unwrap();
    let cancel = CancelToken::new();
    let signal = cancel.clone();
    let result = run_hash(file.path().into(), Algo::Sha256, cancel, move |bytes| {
        if bytes > 0 {
            signal.cancel();
        }
    });
    assert_eq!(result.unwrap_err().kind, FsErrorKind::Cancelled);
}

#[test]
fn hash_missing_file_returns_not_found() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("missing");
    let error = run_hash(path.clone(), Algo::Crc32, CancelToken::new(), |_| {}).unwrap_err();
    assert_eq!(error.kind, FsErrorKind::NotFound);
    assert_eq!(error.location.unwrap().as_str(), path.to_str().unwrap());
}
