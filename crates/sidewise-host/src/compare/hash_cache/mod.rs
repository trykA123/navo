//! Persistent compare hash cache: the record format, the file store and its compaction.
//! Lets a repeat compare skip re-reading files whose size, time and identity are unchanged.

mod compact;
pub mod file;
pub mod policy;
pub mod record;

#[cfg(test)]
mod clear_tests;
#[cfg(test)]
mod file_tests;
#[cfg(test)]
mod flush_tests;
#[cfg(test)]
mod record_tests;
#[cfg(test)]
mod reuse_tests;
#[cfg(test)]
mod timing_tests;
