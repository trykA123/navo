//! The portable file-diff leaves: the hooks, byte worker, image probe, pair loader and side glue.

pub mod bytes;
pub mod cards;
pub mod convert;
pub mod copy_port;
pub mod digest;
pub mod hooks;
pub mod image;
pub mod large;
pub mod load;
pub mod pair;
pub mod sides;
pub mod sources;
pub mod stream;
pub mod viewed;
pub mod viewed_file;
pub mod write;

#[cfg(test)]
mod sources_tests;
#[cfg(test)]
mod tests;
