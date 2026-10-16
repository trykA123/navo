#![forbid(unsafe_code)]

mod error;
pub mod json;
pub mod markdown;
mod preview;
mod source;
pub mod table;
pub mod tree;
pub mod xml;
pub mod yaml;

pub use error::ParseError;
pub use source::{ReadAt, Watch};
pub use tree::{
    Exported, Filtered, Flavor, Key, Lazy, NodeId, NodeKind, Outline, Row, Span, Step, Syntax, Tree,
};

pub const TREE_LIMIT: u64 = 64 * 1024 * 1024;
pub const SMALL_LIMIT: u64 = 16 * 1024 * 1024;
