#![forbid(unsafe_code)]

mod fake;
mod provider;

pub use fake::{FakeFs, FakeOp, Fault};
pub use provider::{
    Attributes, CancelToken, Capabilities, EntryBatch, EntryKind, FsEntry, ListSink, Provider,
};
