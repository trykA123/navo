//! Windows-only probes that the installer VM tables call through `--tester-probe`.

#![cfg(windows)]

pub mod chain;
pub mod discovery;
pub mod elevate_launch;
pub mod elevated_create;
pub mod facts;
pub mod helper_checks;
mod parent_acl;
pub mod pin;
pub mod shell;
pub mod shell_spawn;
mod shell_token;
