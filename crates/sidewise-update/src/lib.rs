//! Sidewise's update check: which release is newer, the GitHub feed and the request. No GUI.
// The WinHTTP FFI and its unsafe code live in `sidewise-winhttp`.
#![forbid(unsafe_code)]

pub mod check;
pub mod feed;
pub mod fetch;
pub mod version;
