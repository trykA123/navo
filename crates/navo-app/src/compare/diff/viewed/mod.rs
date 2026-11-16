//! Navo's cache-backed Viewed store beside the moved Viewed marks.

mod store;

pub use sidewise_host::compare::diff::viewed::*;
#[cfg(test)]
pub(crate) use store::ViewedStore;
pub(crate) use store::opener;
