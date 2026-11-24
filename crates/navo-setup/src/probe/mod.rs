//! Windows-only probes that the installer VM tables call through `--tester-probe`.

#![cfg(windows)]

pub mod chain;
pub mod discovery;
pub mod elevate_launch;
pub mod facts;
mod parent_acl;
pub mod pin;
pub mod shell;
mod shell_token;
