use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

/// Parsed command line. No flag at all means: show the folder picker.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Args {
    pub folder: Option<PathBuf>,
    pub quick: bool,
    pub out: Option<PathBuf>,
}

/// Why the command line was refused. Never holds the offending text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgsError {
    UnknownArgument,
    MissingValue,
    Repeated,
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownArgument => "args:unknown-argument",
            Self::MissingValue => "args:missing-value",
            Self::Repeated => "args:repeated",
        })
    }
}

impl std::error::Error for ArgsError {}

/// Parses the arguments after the program name.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Args, ArgsError> {
    let mut parsed = Args::default();
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        match flag.to_str() {
            Some("--quick") => set_once(&mut parsed.quick, true)?,
            Some("--folder") => set_path(&mut parsed.folder, args.next())?,
            Some("--out") => set_path(&mut parsed.out, args.next())?,
            _ => return Err(ArgsError::UnknownArgument),
        }
    }
    Ok(parsed)
}

fn set_once(slot: &mut bool, value: bool) -> Result<(), ArgsError> {
    if *slot {
        return Err(ArgsError::Repeated);
    }
    *slot = value;
    Ok(())
}

fn set_path(slot: &mut Option<PathBuf>, value: Option<OsString>) -> Result<(), ArgsError> {
    let value = value.ok_or(ArgsError::MissingValue)?;
    if value.is_empty() || value.to_str().is_some_and(|text| text.starts_with("--")) {
        return Err(ArgsError::MissingValue);
    }
    if slot.is_some() {
        return Err(ArgsError::Repeated);
    }
    *slot = Some(PathBuf::from(value));
    Ok(())
}
