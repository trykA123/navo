pub mod diff;
pub mod intraline;
pub mod normalise;
pub mod pair;
pub mod side;

pub use diff::{Alignment, Fallback, Hunk, Row, diff_lines};
pub use intraline::{
    INTRALINE_DEADLINE, INTRALINE_MAX_CHARS, Intraline, IntralineFallback, LinePair, Span,
    SpanSide, intraline,
};
pub use normalise::{Options, Whitespace, normalise};
pub use pair::{PAIR_THRESHOLD, Pair, Pairs, Subhunk, pair_lines};
pub use side::{DIFF_TOKEN_LIMIT, Large, Line, LineEnding, TextSide, Whole, content_class, detect};
