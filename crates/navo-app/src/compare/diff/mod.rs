//! File diff backend: pair loading, UI conversion, pair control and copying one change (D24, D29).
//!
//! Every step runs on an app thread; the UI receives owned snapshots tagged by request id.

mod bytes;
mod cards;
mod convert;
mod copy;
mod copy_hook;
mod copy_many;
mod hooks;
mod image;
mod large;
mod load;
mod pair;
mod sheet_hook;
mod sides;
mod sources;
mod stream;
mod viewed;

pub use hooks::{DiffBackend, diff_backend};

#[cfg(test)]
mod tests;
