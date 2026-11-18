//! Paged text and hex pieces: line models, row drawing and the byte scrollbar.
pub mod lines;
pub mod row_view;
#[cfg(feature = "test-support")]
pub mod run_probe;
pub mod scroll;
pub mod spans;
mod syntax_runs;

pub use lines::{HexLine, PagedKind};
pub use row_view::hex_row_tinted;
pub use scroll::ByteScroll;
