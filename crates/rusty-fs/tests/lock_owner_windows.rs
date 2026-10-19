#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test on a broken fixture"
)]

mod support;

use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use rusty_core::FsErrorKind;
use rusty_fs::{LocalFs, ProviderOps};
use support::location_of;

struct LockOwner(Child);

impl Drop for LockOwner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_until_locked(owner: &mut LockOwner) {
    let stdout = owner.0.stdout.take().unwrap();
    let mut line = String::new();
    BufReader::new(stdout).read_line(&mut line).unwrap();
    assert_eq!(
        line.trim(),
        "locked",
        "the lock owner could not open the file"
    );
}

#[test]
fn sharing_violation_names_the_child_holding_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("locked.txt");
    fs::write(&path, b"locked").unwrap();
    let mut owner = LockOwner(
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$ErrorActionPreference='Stop'; $f=[IO.File]::Open($env:FERRITE_LOCK_TEST_PATH,'Open','ReadWrite','None'); [Console]::Out.WriteLine('locked'); [Console]::Out.Flush(); Start-Sleep 30",
            ])
            .env("FERRITE_LOCK_TEST_PATH", &path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    wait_until_locked(&mut owner);

    let failure = LocalFs::new().remove_file(&location_of(&path)).unwrap_err();
    assert_eq!(failure.kind, FsErrorKind::SharingViolation);
    let holder = failure
        .holders
        .iter()
        .find(|holder| holder.pid == owner.0.id())
        .expect("the error must identify the child holding the file");
    assert!(!holder.name.is_empty());
    assert!(failure.message().contains(&holder.name));
}
