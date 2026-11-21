//! Address parsing for remote https Git sides, with no network code and no gix.
#![forbid(unsafe_code)]

pub mod address;

pub use address::{AddressError, RemoteRev, RemoteUrl};
