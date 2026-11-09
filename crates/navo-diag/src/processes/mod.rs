//! Process scanning: whether any process runs an image under an install folder.

use std::io;
use std::path::Path;

#[cfg(windows)]
mod windows;

#[cfg(test)]
mod tests;

/// Whether another process runs an image under `root`. Any doubt is `Err`.
#[cfg(windows)]
pub fn any_under(root: &Path) -> io::Result<bool> {
    windows::any_under(root)
}

/// Off Windows no process scan exists; the `Unsupported` error means "treat as running".
#[cfg(not(windows))]
pub fn any_under(_root: &Path) -> io::Result<bool> {
    Err(io::ErrorKind::Unsupported.into())
}

/// An image we cannot read blocks the apply when it runs in this session and its name is this
/// image's name or starts with `navo`; the index service runs in session 0 as `navo-index.exe`.
#[cfg_attr(
    not(any(windows, test)),
    allow(
        dead_code,
        reason = "the Windows scan and the tests are the only callers"
    )
)]
pub(crate) fn unreadable_counts(name: &str, own: &str, same_session: bool) -> bool {
    let name = name.to_lowercase();
    same_session && (name == own || name.starts_with("navo"))
}

/// This process and its children (the PDF helper) never block a restart.
#[cfg_attr(
    not(any(windows, test)),
    allow(
        dead_code,
        reason = "the Windows scan and the tests are the only callers"
    )
)]
pub(crate) fn is_other(pid: u32, parent: u32, own: u32) -> bool {
    pid != 0 && pid != own && parent != own
}
