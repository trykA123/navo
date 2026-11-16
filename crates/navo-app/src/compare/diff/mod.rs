//! File diff backend: pair loading, UI conversion, pair control and copying one change (D24, D29).
//!
//! Every step runs on an app thread; the UI receives owned snapshots tagged by request id.

mod copy;
mod copy_hook;
mod copy_many;
mod navo_backend;
mod sheet_hook;
mod sides;
mod viewed;

pub use copy_port::CopyHost;
pub use hooks::DiffBackend;
pub use navo_backend::{diff_backend, navo_compare_dirs};
pub use sidewise_host::compare::diff::{
    bytes, cards, convert, copy_port, digest, hooks, image, large, load, pair, sources, stream,
};

#[cfg(test)]
mod tests;
