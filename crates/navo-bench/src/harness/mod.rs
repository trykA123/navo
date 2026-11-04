//! Run harness: arguments, folder choice, tester probe, statistics and the phase driver.

pub mod args;
pub mod phase;
pub mod picker;
pub mod probe;
pub mod progress;
pub mod start;
pub mod stats;

#[cfg(test)]
mod tests;
