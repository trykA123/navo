//! IW17 command line: silent installs, version, payload hashes and strict argument errors.

use std::path::PathBuf;

#[cfg(feature = "tester")]
use navo_update::Version;

use crate::payload::PayloadError;

/// The two per-run log paths created after parsing and payload loading.
#[derive(Clone, Debug)]
pub struct LogPaths {
    pub wizard: PathBuf,
    pub velopack: PathBuf,
}

/// A parsed command: one of the three things the exe can do.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Version,
    Hashes { setup: bool, package: bool },
    Silent(Silent),
}

/// One silent run's folder and shortcut choices.
#[derive(Debug, PartialEq, Eq)]
pub struct Silent {
    pub install_to: Option<PathBuf>,
    pub desktop: bool,
    pub start_menu: bool,
}

/// A tester payload triple, accepted only with the `tester` feature.
#[cfg(feature = "tester")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadFiles {
    pub setup: PathBuf,
    pub package: PathBuf,
    pub version: Version,
}

/// The tester payload carried into execution.
#[cfg(feature = "tester")]
#[derive(Clone, Default)]
pub struct PayloadSelection {
    pub files: Option<PayloadFiles>,
}

/// Ordinary builds carry no tester payload.
#[cfg(not(feature = "tester"))]
#[derive(Default)]
pub struct PayloadSelection;

/// Parsed arguments plus the optional tester payload.
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed {
    pub command: Command,
    #[cfg(feature = "tester")]
    pub files: Option<PayloadFiles>,
}

impl Parsed {
    /// Returns the tester payload triple, or an empty selection in ordinary builds.
    pub fn payload_selection(&self) -> PayloadSelection {
        PayloadSelection {
            #[cfg(feature = "tester")]
            files: self.files.clone(),
        }
    }
}

/// A bad argument, payload, discovery or host; each maps to an IW17 code.
#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Payload(PayloadError),
    Discovery,
    Elevated,
    AlreadyInstalled(PathBuf),
    Folder(String),
    Logs(String),
    NoHost,
}

impl CliError {
    /// Returns IW17's silent exit code for this failure.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) | Self::NoHost => 2,
            Self::Payload(_) => 9,
            Self::Discovery => 6,
            Self::Elevated => 7,
            Self::AlreadyInstalled(_) | Self::Folder(_) => 3,
            Self::Logs(_) => 5,
        }
    }

    /// Returns the stderr line for this failure.
    pub fn message(&self) -> String {
        match self {
            Self::Usage(message) => message.clone(),
            Self::Payload(error) => error.to_string(),
            Self::Discovery => "Setup could not read the installed copy.".to_owned(),
            Self::Elevated => "start Setup normally, without Run as administrator".to_owned(),
            Self::AlreadyInstalled(root) => format!(
                "Navo is already installed in `{}`; Setup updates it there",
                root.display()
            ),
            Self::Folder(message) | Self::Logs(message) => message.clone(),
            Self::NoHost => "Silent mode is available only on Windows.".to_owned(),
        }
    }
}

/// Encodes one SHA-256 as 64 lowercase hex characters.
pub fn hex_hash(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;

#[path = "cli_messages.rs"]
mod messages;
#[path = "cli_parse.rs"]
mod parse_flags;
#[path = "cli_run.rs"]
mod run;
pub use parse_flags::parse;

pub use run::run;
