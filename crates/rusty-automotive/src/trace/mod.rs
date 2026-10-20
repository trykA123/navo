mod asc;
mod decode;
mod index;
mod line;
mod sniff;
mod text;
mod trc;

pub use decode::Plan;
pub use index::{IdStat, Kinds, MAX_IDS, Trace, TraceProblem, index_trace};
pub use sniff::{Evidence, sniff_trace};

pub const TRACE_EXTENSIONS: [&str; 2] = ["asc", "trc"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TraceFormat {
    #[default]
    Asc,
    Trc(Option<&'static str>),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Base {
    #[default]
    Hex,
    Dec,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Notation {
    pub base: Base,
    pub relative: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanId {
    pub raw: u32,
    pub extended: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Rx,
    Tx,
    TxRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Can,
    Fd { brs: bool, esi: bool },
    Remote,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub channel: u16,
    pub id: Option<CanId>,
    pub direction: Option<Direction>,
    pub kind: FrameKind,
    pub dlc: u8,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Frame(Frame),
    Other(String),
    Malformed { text: String, reason: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub row: u64,
    pub line: u64,
    pub time_ns: Option<u64>,
    pub body: Body,
}
