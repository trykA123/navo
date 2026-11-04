//! The crate's only process start; `tests/spawn_allows.rs` keeps it that way.

use std::ffi::OsString;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

const CREATE_SUSPENDED: u32 = 0x0000_0004;

/// Starts `exe` with its main thread suspended, so it runs only once it is inside the job.
#[expect(
    clippy::disallowed_methods,
    reason = "the one reviewed process start; arguments come from probe::args only"
)]
pub(super) fn suspended(exe: &Path, args: &[OsString]) -> io::Result<Child> {
    Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_SUSPENDED)
        .spawn()
}
