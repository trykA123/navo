//! Reads local Git repositories for Sidewise, in process and read-only. Never starts a program.
#![forbid(unsafe_code)]

pub mod checkout;
pub mod checkout_config;
mod config_scan;
#[cfg(any(test, feature = "test-support"))]
pub mod fixture;
pub mod index;
pub mod location;
mod lookup;
mod pointers;
#[cfg(test)]
mod remote_refs_tests;
pub mod repo;
pub mod side;
