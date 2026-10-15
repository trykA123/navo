use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Open,
    View(PathBuf),
    Version,
    DiagReport(PathBuf),
}

pub const USAGE: &str = "usage: ferrite [--version | --view <file> | --diag-report <file.zip>]";

impl Command {
    pub fn uses_console(&self) -> bool {
        !matches!(self, Self::Open | Self::View(_))
    }
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, &'static str> {
    let mut args = args.into_iter();
    let (first, second) = (args.next(), args.next());
    if args.next().is_some() {
        return Err(USAGE);
    }
    match (first, second) {
        (None, None) => Ok(Command::Open),
        (Some(flag), None) if flag == "--version" => Ok(Command::Version),
        (Some(flag), Some(path)) if flag == "--diag-report" && !path.is_empty() => {
            Ok(Command::DiagReport(path.into()))
        }
        (Some(flag), Some(path)) if flag == "--view" && !path.is_empty() => {
            Ok(Command::View(path.into()))
        }
        _ => Err(USAGE),
    }
}

#[cfg(test)]
mod tests;
