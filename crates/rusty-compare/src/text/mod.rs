pub mod normalise;
pub mod side;

pub use normalise::{Options, Whitespace, normalise};
pub use side::{Large, Line, LineEnding, TextSide, Whole, content_class, detect};
