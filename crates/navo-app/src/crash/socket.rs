//! Where the native crash client puts its server socket: the temp folder, or `<root>/data/run`
//! for a portable copy (NV9a), so a portable Navo writes nothing outside its folder.

use std::io;
use std::path::{Path, PathBuf};

/// Returns the socket path for process `pid`: in `portable_run` (created when missing) or else in
/// `temp`. Fails when the portable folder cannot be created; it never falls back to `temp`.
pub(super) fn path(portable_run: Option<&Path>, temp: &Path, pid: u32) -> io::Result<PathBuf> {
    let name = format!("navo-crash-{pid}.sock");
    let Some(run) = portable_run else {
        return Ok(temp.join(name));
    };
    std::fs::create_dir_all(run)?;
    Ok(run.join(name))
}

#[cfg(test)]
mod tests {
    use super::path;

    #[test]
    fn a_portable_copy_puts_the_crash_socket_in_data_run() {
        let root = tempfile::tempdir().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let run = root.path().join("data").join("run");
        let socket = path(Some(&run), temp.path(), 42).unwrap();
        assert_eq!(socket, run.join("navo-crash-42.sock"));
        assert!(run.is_dir());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn an_installed_copy_keeps_the_crash_socket_in_temp() {
        let temp = tempfile::tempdir().unwrap();
        let socket = path(None, temp.path(), 7).unwrap();
        assert_eq!(socket, temp.path().join("navo-crash-7.sock"));
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_blocked_portable_folder_fails_instead_of_using_temp() {
        let root = tempfile::tempdir().unwrap();
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("data"), b"").unwrap();
        let run = root.path().join("data").join("run");
        assert!(path(Some(&run), temp.path(), 1).is_err());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
