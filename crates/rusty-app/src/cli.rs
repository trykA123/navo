use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Version,
    DiagReport(PathBuf),
}

pub const USAGE: &str = "usage: ferrite [--version | --diag-report <file.zip>]";

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, &'static str> {
    let mut args = args.into_iter();
    let (first, second) = (args.next(), args.next());
    if args.next().is_some() {
        return Err(USAGE);
    }
    match (first, second) {
        (None, None) => Ok(Command::Version),
        (Some(flag), None) if flag == "--version" => Ok(Command::Version),
        (Some(flag), Some(path)) if flag == "--diag-report" && !path.is_empty() => {
            Ok(Command::DiagReport(path.into()))
        }
        _ => Err(USAGE),
    }
}

#[cfg(test)]
mod tests;
