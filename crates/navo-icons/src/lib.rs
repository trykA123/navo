//! Icon ids and their forms, shared by the button bar, the editor and the packer.

#![forbid(unsafe_code)]

/// Bounded file readers for untrusted icon inputs.
pub mod io;
/// Reads and writes `NVIC1` icon packs.
pub mod pack;
/// Security-hardened raster icon decoder, resizer, and PNG re-encoder.
pub mod raster;
/// Security-hardened SVG normaliser with strict allowlist XML pre-parsing.
pub mod sanitise;
/// Reads upstream icon tarballs into normalised icon-set drafts.
pub mod sources;
/// Parses and writes icon specs from user text and inline TOML tables.
pub mod spec;
