use std::fs;
use std::path::{Path, PathBuf};

use rusty_core::Location;

use super::{head, paged};

fn exact(path: &Path) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!(r"\\?\{}", path.display()))
    } else {
        path.to_owned()
    }
}

fn at(path: &Path) -> Location {
    Location::new(path.to_string_lossy().into_owned()).unwrap()
}

#[test]
fn head_and_paged_read_a_trailing_dot_name_as_itself() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(exact(&dir.path().join("trace")), b"plain").unwrap();
    fs::write(exact(&dir.path().join("trace.")), b"dotted bytes").unwrap();
    let dotted = at(&dir.path().join("trace."));
    let first = head(&dotted).unwrap();
    assert_eq!(first.bytes, b"dotted bytes");
    assert_eq!(first.len, 12);
    assert_eq!(paged(&dotted).unwrap().source.len(), 12);
}
