use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Open(Start),
    Version,
    DiagReport(PathBuf),
    CrashTest(PathBuf),
    PdfHelper,
    IndexService(crate::index_service::Action),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Start {
    pub(crate) experimental_retention: bool,
    pub(crate) path: Option<PathBuf>,
    pub(crate) view: Option<PathBuf>,
    pub(crate) profile: Option<PathBuf>,
    pub(crate) metrics: Option<PathBuf>,
    pub(crate) left: Option<PathBuf>,
    pub(crate) right: Option<PathBuf>,
}

pub(crate) const USAGE: &str = "usage: navo [--version | --diag-report <file.zip> | [--view <file>] [--left <folder>] [--right <folder>] [--profile <folder>] [--metrics <file.json>] [<path>]]";

const STANDALONE: [&str; 5] = [
    "--version",
    "--diag-report",
    "--pdf-helper",
    "--crash-test",
    "--index-service",
];
const WINDOW_FLAGS: [&str; 5] = ["--view", "--profile", "--metrics", "--left", "--right"];

impl Command {
    pub(crate) fn uses_console(&self) -> bool {
        match self {
            Self::Open(start) => start.is_metrics(),
            Self::PdfHelper => false,
            Self::IndexService(_) => false,
            Self::Version | Self::DiagReport(_) | Self::CrashTest(_) => true,
        }
    }
}

impl Start {
    pub(crate) fn is_metrics(&self) -> bool {
        self.metrics.is_some()
    }

    fn slot(&mut self, flag: &str) -> Option<&mut Option<PathBuf>> {
        match flag {
            "--view" => Some(&mut self.view),
            "--profile" => Some(&mut self.profile),
            "--metrics" => Some(&mut self.metrics),
            "--left" => Some(&mut self.left),
            "--right" => Some(&mut self.right),
            _ => None,
        }
    }
}

pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    read(args.into_iter()).map_err(|problem| format!("navo: {problem}\n{USAGE}"))
}

fn read(mut args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let mut start = Start::default();
    let mut is_first = true;
    while let Some(arg) = args.next() {
        let flag = arg.to_string_lossy();
        if STANDALONE.contains(&flag.as_ref()) {
            if !is_first {
                return Err(format!("{flag} stands alone"));
            }
            return standalone(&flag, args);
        }
        is_first = false;
        if flag == "--experimental-retention" {
            if std::mem::replace(&mut start.experimental_retention, true) {
                return Err(format!("{flag} given twice"));
            }
        } else if let Some(slot) = start.slot(&flag) {
            if slot.is_some() {
                return Err(format!("{flag} given twice"));
            }
            *slot = Some(value(&flag, args.next())?);
        } else if flag.starts_with('-') || arg.is_empty() {
            return Err(format!("unknown argument {flag}"));
        } else if start.path.replace(PathBuf::from(arg)).is_some() {
            return Err("only one path is accepted".into());
        }
    }
    Ok(Command::Open(start))
}

fn standalone(flag: &str, mut rest: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let command = match flag {
        "--version" => Command::Version,
        "--pdf-helper" => Command::PdfHelper,
        "--index-service" => Command::IndexService(crate::index_service::Action::parse(
            rest.next()
                .as_deref()
                .ok_or("--index-service needs a mode")?,
        )?),
        "--crash-test" => Command::CrashTest(value(flag, rest.next())?),
        _ => Command::DiagReport(value(flag, rest.next())?),
    };
    let Some(extra) = rest.next() else {
        return Ok(command);
    };
    let extra = extra.to_string_lossy();
    if STANDALONE.contains(&extra.as_ref())
        || WINDOW_FLAGS.contains(&extra.as_ref())
        || extra == "--experimental-retention"
    {
        Err(format!("{flag} stands alone"))
    } else {
        Err(format!("unknown argument {extra}"))
    }
}

fn value(flag: &str, next: Option<OsString>) -> Result<PathBuf, String> {
    next.filter(|value| !value.is_empty() && !value.to_string_lossy().starts_with("--"))
        .map(PathBuf::from)
        .ok_or_else(|| format!("{flag} needs a value"))
}

#[cfg(test)]
mod tests;
