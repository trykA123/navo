//! Icon ids and their forms, shared by the button bar, the editor and the packer.

#![forbid(unsafe_code)]

/// Reads and writes `NVIC1` icon packs.
pub mod pack;
/// Parses and writes icon specs from user text and inline TOML tables.
pub mod spec;
