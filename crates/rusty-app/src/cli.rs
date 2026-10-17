use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Open(Start),
    Version,
    DiagReport(PathBuf),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Start {
    pub view: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    pub metrics: Option<PathBuf>,
    pub left: Option<PathBuf>,
    pub right: Option<PathBuf>,
}

pub const USAGE: &str = "usage: ferrite [--version | --diag-report <file.zip> | [--view <file>] [--left <folder>] [--right <folder>] [--profile <folder>] [--metrics <file.json>]]";

impl Command {
    pub fn uses_console(&self) -> bool {
        false
    }
}

impl Start {
    pub fn is_metrics(&self) -> bool {
        false
    }
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let _ = args.into_iter();
    Err(USAGE.to_owned())
}

#[cfg(test)]
mod tests;
