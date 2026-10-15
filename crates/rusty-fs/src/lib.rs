#![forbid(unsafe_code)]

mod fake;
pub mod hostile_names;
mod provider;

pub use fake::{FakeFs, FakeOp, Fault};
pub use provider::{
    Attributes, CancelToken, Capabilities, EntryBatch, EntryKind, FsEntry, ListSink, Provider,
};
