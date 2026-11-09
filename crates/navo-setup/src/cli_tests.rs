//! Linux and Windows tests for IW17 parsing and silent exit codes.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use navo_core::{FileIdentity, FileIdentityKind};
use navo_update::parse_version;

use super::*;
use crate::flow::Failure;
use crate::mode::{Discovery, Install, Mode, SqVersion, parse_sq_version};
use crate::payload::Payload;
use crate::runner::{InstallError, Registry, Verified};

use crate::runner_fake::FakeHost;

#[cfg(windows)]
const ROOT: &str = "C:/Programs/Navo";
#[cfg(not(windows))]
const ROOT: &str = "/Programs/Navo";
const LOG: &str = r"C:\Temp\NavoSetup-logs\test-velopack.log";
const VERSION: &str = "0.4.0";

/// Builds native arguments from ASCII test values.
fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

/// Parses arguments that must succeed.
fn parsed(values: &[&str]) -> Parsed {
    parse(&args(values)).expect("arguments parse")
}

/// Parses arguments that must fail with exit 2.
fn usage_error(values: &[&str]) {
    let error = parse(&args(values)).expect_err("arguments must fail");
    assert_eq!(error.exit_code(), 2, "{values:?}");
}

/// Every flag alone and in combination reaches the expected command.
#[test]
fn each_flag_alone_and_in_combination_parses() {
    assert_eq!(
        parsed(&["--silent"]).command,
        Command::Silent(Silent {
            install_to: None,
            desktop: true,
            start_menu: true,
        })
    );
    assert_eq!(
        parsed(&[
            "--silent",
            "--installto",
            ROOT,
            "--no-desktop",
            "--no-start-menu",
        ])
        .command,
        Command::Silent(Silent {
            install_to: Some(PathBuf::from(ROOT)),
            desktop: false,
            start_menu: false,
        })
    );
    assert_eq!(parsed(&["--version"]).command, Command::Version);
    assert_eq!(
        parsed(&["--payload-sha256"]).command,
        Command::Hashes {
            setup: true,
            package: false
        }
    );
    assert_eq!(
        parsed(&["--payload-sha256", "--package-sha256"]).command,
        Command::Hashes {
            setup: true,
            package: true
        }
    );
}

/// Every malformed or repeated flag exits 2.
#[test]
fn every_bad_argument_exits_2() {
    for values in [
        &["--bogus"][..],
        &["--silent", "--silent"][..],
        &["--version", "--version"][..],
        &["--payload-sha256", "--payload-sha256"][..],
        &["--installto", ROOT][..],
        &["--no-desktop"][..],
        &["--no-start-menu"][..],
        &["--silent", "--installto"][..],
        &["--silent", "--installto", ROOT, "--installto", ROOT][..],
        &["--version", "--silent"][..],
        &["--silent", "--payload-sha256"][..],
    ] {
        usage_error(values);
    }
    usage_error(&[]);
}

/// A non-UTF-8 install path survives parsing unchanged.
#[test]
fn a_non_utf8_install_path_survives_parsing() {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(vec![b'C', b':', b'\\', 0xff]);
        let parsed = parse(&[
            OsString::from("--silent"),
            OsString::from("--installto"),
            path.clone(),
        ])
        .expect("non-UTF-8 path parses");
        let Command::Silent(silent) = parsed.command else {
            panic!("expected silent mode");
        };
        assert_eq!(silent.install_to, Some(PathBuf::from(path)));
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let path = OsString::from_wide(&[b'C' as u16, b':' as u16, b'\\' as u16, 0xd800]);
        let parsed = parse(&[
            OsString::from("--silent"),
            OsString::from("--installto"),
            path.clone(),
        ])
        .expect("lone-surrogate path parses");
        let Command::Silent(silent) = parsed.command else {
            panic!("expected silent mode");
        };
        assert_eq!(silent.install_to, Some(PathBuf::from(path)));
    }
}

/// The hash encoder is lowercase hex with 64 characters.
#[test]
fn hashes_are_lowercase_hex() {
    let text = hex_hash(&[0xab; 32]);
    assert_eq!(text.len(), 64);
    assert_eq!(text, "ab".repeat(32));
}

/// Tester payload flags accept all three or none.
#[cfg(feature = "tester")]
#[test]
fn the_tester_payload_triple_is_all_or_nothing() {
    let parsed = parse(&args(&[
        "--silent",
        "--payload",
        "setup.exe",
        "--payload-package",
        "package.nupkg",
        "--payload-version",
        VERSION,
    ]))
    .expect("complete triple parses");
    let files = parsed.files.expect("files");
    assert_eq!(files.setup, PathBuf::from("setup.exe"));
    assert_eq!(files.package, PathBuf::from("package.nupkg"));
    assert_eq!(files.version, parse_version(VERSION).unwrap());
    for values in [
        &["--silent", "--payload", "setup.exe"][..],
        &[
            "--silent",
            "--payload",
            "setup.exe",
            "--payload-package",
            "package.nupkg",
        ][..],
    ] {
        usage_error(values);
    }
}

#[path = "cli_silent_tests.rs"]
mod silent;
