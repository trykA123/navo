//! Contract tests for NV12e folder-sync quoting: POSIX single quotes, Git Bash path
//! conversion (including rejected drive-relative forms) and WSL `wslpath`.

use std::path::Path;

use crate::prompt::{SyncWait, quote_sync_command};
use crate::shells::ShellKind;

const BAD_NAMES: &[&str] = &[
    "a\nb",
    "a\rb",
    "a\0b",
    "a\x1bb",
    "a\tb",
    "a\u{2028}b",
    "a\u{2029}b",
    "",
];

#[test]
fn posix_shells_quote_with_single_quotes_and_reject_controls() {
    for kind in [ShellKind::Bash, ShellKind::Zsh, ShellKind::Fish] {
        assert_eq!(
            quote_sync_command(kind, Path::new("it's here")),
            Ok(String::from("cd -- 'it'\\''s here'\r")),
            "apostrophe quoting for {kind:?}"
        );
        assert_eq!(
            quote_sync_command(kind, Path::new("/home/a b")),
            Ok(String::from("cd -- '/home/a b'\r")),
            "space quoting for {kind:?}"
        );
        for bad in BAD_NAMES {
            assert_eq!(
                quote_sync_command(kind, Path::new(bad)),
                Err(SyncWait::UnusualName),
                "expected UnusualName for {bad:?} in {kind:?}"
            );
        }
    }
}

#[test]
fn git_bash_converts_windows_paths_then_posix_quotes() {
    assert_eq!(
        quote_sync_command(ShellKind::GitBash, Path::new(r"C:\Users\it's here")),
        Ok(String::from("cd -- '/c/Users/it'\\''s here'\r"))
    );
    assert_eq!(
        quote_sync_command(ShellKind::GitBash, Path::new(r"C:\Users\a b")),
        Ok(String::from("cd -- '/c/Users/a b'\r"))
    );
    assert_eq!(
        quote_sync_command(ShellKind::GitBash, Path::new("c:/Users/a b")),
        Ok(String::from("cd -- '/c/Users/a b'\r"))
    );
    for bad in BAD_NAMES {
        assert_eq!(
            quote_sync_command(ShellKind::GitBash, Path::new(bad)),
            Err(SyncWait::UnusualName),
            "expected UnusualName for {bad:?}"
        );
    }
    // UNC, extended-length and non-local paths are unsupported.
    for bad in [
        r"\\server\share",
        r"\\?\C:\Users",
        "relative/dir",
        "/unix/path",
    ] {
        assert_eq!(
            quote_sync_command(ShellKind::GitBash, Path::new(bad)),
            Err(SyncWait::UnusualName),
            "expected UnusualName for {bad:?}"
        );
    }
}

#[test]
fn git_bash_rejects_drive_relative_and_bare_drive_paths() {
    // `C:foo` means "the current folder of drive C" on Windows; mapping it to `/cfoo`
    // would silently `cd` somewhere the user never chose.
    for bad in [r"C:foo", "C:", r"c:Users", r"C:foo\bar"] {
        assert_eq!(
            quote_sync_command(ShellKind::GitBash, Path::new(bad)),
            Err(SyncWait::UnusualName),
            "expected UnusualName for {bad:?}"
        );
    }
    // A drive root is absolute: `C:\` and `C:/` are the root of `C:`.
    for root in [r"C:\", "C:/"] {
        assert_eq!(
            quote_sync_command(ShellKind::GitBash, Path::new(root)),
            Ok(String::from("cd -- '/c'\r")),
            "wrong conversion for {root:?}"
        );
    }
}

#[test]
fn wsl_sync_uses_wslpath_with_posix_quoting() {
    assert_eq!(
        quote_sync_command(ShellKind::Wsl, Path::new(r"C:\Users\a b")),
        Ok(String::from("cd -- \"$(wslpath -u 'C:\\Users\\a b')\"\r"))
    );
    assert_eq!(
        quote_sync_command(ShellKind::Wsl, Path::new(r"C:\Users\it's here")),
        Ok(String::from(
            "cd -- \"$(wslpath -u 'C:\\Users\\it'\\''s here')\"\r"
        ))
    );
    for bad in BAD_NAMES {
        assert_eq!(
            quote_sync_command(ShellKind::Wsl, Path::new(bad)),
            Err(SyncWait::UnusualName),
            "expected UnusualName for {bad:?}"
        );
    }
}
