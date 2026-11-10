//! File diff backend: pair loading, UI conversion, pair control and copying one change (D24, D29).
//!
//! Every step runs on an app thread; the UI receives owned snapshots tagged by request id.

mod convert;
mod copy;
mod copy_many;
mod hooks;
mod load;
mod pair;
mod sides;
mod stream;
mod viewed;

pub(crate) use hooks::DiffBackend;

#[cfg(test)]
mod tests;
