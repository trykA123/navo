#![forbid(unsafe_code)]

mod error;
pub mod json;
mod preview;
mod source;
pub mod table;
pub mod tree;
pub mod xml;
pub mod yaml;

pub use error::ParseError;
pub use source::{ReadAt, Watch};
pub use tree::{Filtered, Flavor, Key, Lazy, NodeId, NodeKind, Outline, Row, Span, Step, Tree};

pub const TREE_LIMIT: u64 = 64 * 1024 * 1024;
pub const SMALL_LIMIT: u64 = 16 * 1024 * 1024;
