//! Navo's Git and local file sides beside the moved side glue.

mod git;

pub(crate) use git::navo_side_of;
pub use sidewise_host::compare::diff::sides::*;
