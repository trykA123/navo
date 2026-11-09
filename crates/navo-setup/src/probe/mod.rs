//! Windows-only probes that the installer VM tables call through `--tester-probe`.

#![cfg(windows)]

pub mod chain;
pub mod discovery;
pub mod facts;
pub mod pin;
