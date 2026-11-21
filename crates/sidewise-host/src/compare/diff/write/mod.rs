//! The write side of the file diff in the host: copying changes into files through the journal.
//!
//! `request` holds the port of the copy request and its refusals; `compose` writes the copied
//! result into a fill writer; `job` runs one copy through the journal; `host` is the
//! `CopyHost` with its threads and the session's undo stack.

pub mod compose;
pub mod host;
pub mod job;
pub mod request;
