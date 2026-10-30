pub mod diff;
pub mod normalise;
pub mod side;

pub use diff::{Alignment, Fallback, Hunk, Row, diff_lines};
pub use normalise::{Options, Whitespace, normalise};
pub use side::{DIFF_TOKEN_LIMIT, Large, Line, LineEnding, TextSide, Whole, content_class, detect};
