//! Natural string order for file names; re-exported from `navo_core::natural`.
//! Used by the listing sort in `sort.rs`; the tests stay here.

pub use navo_core::natural::{NaturalArena, NaturalRef, natural_cmp};

#[cfg(test)]
mod tests;
