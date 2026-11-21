//! The write journal of Sidewise (plan P1, NC143): every write is logged as a job of
//! checksummed records, so a crash can be rolled back and every write can be undone.

mod error;
mod files;
mod item;
mod job;
mod journal;
pub mod record;
mod recover;
mod rollback;
mod sweep;
mod undo;

#[cfg(test)]
mod codec_strict_tests;
#[cfg(test)]
mod codec_tests;
#[cfg(test)]
mod job_crash_tests;
#[cfg(test)]
mod job_outcome_tests;
#[cfg(test)]
mod job_tests;
#[cfg(test)]
mod noids_tests;
#[cfg(test)]
mod recover_crash_tests;
#[cfg(test)]
mod recover_race_tests;
#[cfg(test)]
mod recover_tests;
#[cfg(test)]
mod rollback_crash_tests;
#[cfg(test)]
mod swap_tests;
#[cfg(test)]
mod time_tests;

pub use error::{JournalError, WriteError};
pub use item::{Expect, Fill};
pub use job::Job;
pub use journal::Journal;
pub use record::{ItemMode, JobId, JobKind, Log, Record, decode, encode};
pub use recover::Recovered;
pub use rollback::Rolled;
pub use sweep::{CAP_BYTES, KEEP_MS, Swept};
pub use undo::UndoError;
