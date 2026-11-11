// Tests for the default root: which roots count as the default one under the known parent.

use std::ffi::OsStr;
use std::path::Path;

use super::{is_default_root, under};
use crate::placement::NameEq;

/// ASCII-only name comparison for the pure tests; Windows supplies the ordinal one.
struct AsciiNames;

impl NameEq for AsciiNames {
    fn same(&self, a: &OsStr, b: &OsStr) -> bool {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
}

/// An absolute parent for the platform the test runs on.
fn parent() -> &'static Path {
    #[cfg(windows)]
    return Path::new(r"C:\Users\U\AppData\Local\Programs");
    #[cfg(not(windows))]
    return Path::new("/Users/U/AppData/Local/Programs");
}

/// The default root is the parent plus `Navo`, and it counts as the default.
#[test]
fn the_default_root_is_recognised() {
    assert!(is_default_root(&under(parent()), parent(), &AsciiNames));
}

/// Letter case follows the injected name rule.
#[test]
fn the_default_root_ignores_letter_case() {
    let lower = under(parent()).to_string_lossy().to_lowercase();
    assert!(is_default_root(Path::new(&lower), parent(), &AsciiNames));
}

/// A typed custom root, the parent itself, a deeper folder and another parent never count.
#[test]
fn other_roots_are_not_the_default() {
    let names = AsciiNames;
    assert!(!is_default_root(&parent().join("Other"), parent(), &names));
    assert!(!is_default_root(parent(), parent(), &names));
    assert!(!is_default_root(
        &under(parent()).join("x"),
        parent(),
        &names
    ));
    assert!(!is_default_root(
        &under(parent().parent().expect("has a parent")),
        parent(),
        &names
    ));
}
