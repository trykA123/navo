//! Argument parsing for IW17: flags, repeats, values and the tester payload triple.

use std::ffi::OsString;
use std::path::PathBuf;

#[cfg(feature = "tester")]
use navo_update::Version;

#[cfg(feature = "tester")]
use crate::cli::PayloadFiles;
use crate::cli::{CliError, Command, Parsed, Silent};

/// Parses IW17's flags over native arguments; every unknown or repeated flag is exit 2.
pub fn parse(args: &[OsString]) -> Result<Parsed, CliError> {
    let mut flags = Flags::default();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        index += 1;
        if parse_flag(arg, &mut flags)? {
            index = parse_value(arg, args, index, &mut flags)?;
        }
    }
    flags.finish()
}

/// The flag state collected while walking the arguments.
#[derive(Default)]
struct Flags {
    silent: bool,
    install_to: Option<PathBuf>,
    no_desktop: bool,
    no_start_menu: bool,
    version: bool,
    setup_hash: bool,
    package_hash: bool,
    #[cfg(feature = "tester")]
    payload_setup: Option<PathBuf>,
    #[cfg(feature = "tester")]
    payload_package: Option<PathBuf>,
    #[cfg(feature = "tester")]
    payload_version: Option<Version>,
}

/// Parses one flag; returns whether it takes a value.
fn parse_flag(arg: &OsString, flags: &mut Flags) -> Result<bool, CliError> {
    if arg == "--silent" {
        set_once(&mut flags.silent, "--silent")?;
        Ok(false)
    } else if arg == "--installto" {
        if flags.install_to.is_some() {
            return Err(usage("--installto was given more than once"));
        }
        Ok(true)
    } else if arg == "--no-desktop" {
        set_once(&mut flags.no_desktop, "--no-desktop")?;
        Ok(false)
    } else if arg == "--no-start-menu" {
        set_once(&mut flags.no_start_menu, "--no-start-menu")?;
        Ok(false)
    } else if arg == "--version" {
        set_once(&mut flags.version, "--version")?;
        Ok(false)
    } else if arg == "--payload-sha256" {
        set_once(&mut flags.setup_hash, "--payload-sha256")?;
        Ok(false)
    } else if arg == "--package-sha256" {
        set_once(&mut flags.package_hash, "--package-sha256")?;
        Ok(false)
    } else {
        parse_tester(arg, flags)
    }
}

/// Parses one tester-only payload flag.
#[cfg(feature = "tester")]
fn parse_tester(arg: &OsString, flags: &mut Flags) -> Result<bool, CliError> {
    if arg == "--payload" {
        if flags.payload_setup.is_some() {
            return Err(usage("--payload was given more than once"));
        }
        Ok(true)
    } else if arg == "--payload-package" {
        if flags.payload_package.is_some() {
            return Err(usage("--payload-package was given more than once"));
        }
        Ok(true)
    } else if arg == "--payload-version" {
        if flags.payload_version.is_some() {
            return Err(usage("--payload-version was given more than once"));
        }
        Ok(true)
    } else {
        Err(usage(&format!(
            "unknown argument `{}`",
            arg.to_string_lossy()
        )))
    }
}

/// Rejects any unknown tester-only flag in ordinary builds.
#[cfg(not(feature = "tester"))]
fn parse_tester(arg: &OsString, _flags: &mut Flags) -> Result<bool, CliError> {
    Err(usage(&format!(
        "unknown argument `{}`",
        arg.to_string_lossy()
    )))
}

/// Reads one option value, preserving a non-UTF-8 `OsString` path.
fn parse_value(
    flag: &OsString,
    args: &[OsString],
    index: usize,
    flags: &mut Flags,
) -> Result<usize, CliError> {
    let value = args
        .get(index)
        .ok_or_else(|| usage(&format!("{} needs a value", flag.to_string_lossy())))?;
    let next = index + 1;
    if flag == "--installto" {
        flags.install_to = Some(PathBuf::from(value));
    } else {
        parse_payload_value(flag, value, flags)?;
    }
    Ok(next)
}

/// Stores one tester payload value or rejects it outside tester builds.
#[cfg(feature = "tester")]
fn parse_payload_value(
    flag: &OsString,
    value: &OsString,
    flags: &mut Flags,
) -> Result<(), CliError> {
    if flag == "--payload" {
        flags.payload_setup = Some(PathBuf::from(value));
    } else if flag == "--payload-package" {
        flags.payload_package = Some(PathBuf::from(value));
    } else if flag == "--payload-version" {
        let text = value
            .to_str()
            .ok_or_else(|| usage("--payload-version must be valid UTF-8"))?;
        flags.payload_version = Some(
            navo_update::parse_version(text)
                .map_err(|_| usage("--payload-version is not a semantic version"))?,
        );
    }
    Ok(())
}

/// Rejects a payload value outside tester builds.
#[cfg(not(feature = "tester"))]
fn parse_payload_value(
    _flag: &OsString,
    _value: &OsString,
    _flags: &mut Flags,
) -> Result<(), CliError> {
    Err(usage("payload flags need a tester build"))
}

/// Rejects a flag that was already present.
fn set_once(flag: &mut bool, name: &str) -> Result<(), CliError> {
    if *flag {
        return Err(usage(&format!("{name} was given more than once")));
    }
    *flag = true;
    Ok(())
}

impl Flags {
    /// Validates combinations and builds the parsed command.
    fn finish(self) -> Result<Parsed, CliError> {
        let files = self.payload_files()?;
        let scripted =
            self.silent || self.install_to.is_some() || self.no_desktop || self.no_start_menu;
        if self.version {
            if scripted || self.setup_hash || self.package_hash || files.is_some() {
                return Err(usage("--version cannot be combined with other flags"));
            }
            return Ok(Parsed {
                command: Command::Version,
                #[cfg(feature = "tester")]
                files,
            });
        }
        if self.setup_hash || self.package_hash {
            if scripted {
                return Err(usage(
                    "hash flags cannot be combined with --silent or its flags",
                ));
            }
            return Ok(Parsed {
                command: Command::Hashes {
                    setup: self.setup_hash,
                    package: self.package_hash,
                },
                #[cfg(feature = "tester")]
                files,
            });
        }
        if !self.silent {
            if self.install_to.is_some() || self.no_desktop || self.no_start_menu {
                return Err(usage(
                    "--installto, --no-desktop and --no-start-menu need --silent",
                ));
            }
            return Err(usage(
                "no command; use --silent, --version, --payload-sha256 or --package-sha256",
            ));
        }
        Ok(Parsed {
            command: Command::Silent(Silent {
                install_to: self.install_to,
                desktop: !self.no_desktop,
                start_menu: !self.no_start_menu,
            }),
            #[cfg(feature = "tester")]
            files,
        })
    }

    /// Accepts all three tester payload flags or none; a partial set is usage.
    #[cfg(feature = "tester")]
    fn payload_files(&self) -> Result<Option<PayloadFiles>, CliError> {
        match (
            &self.payload_setup,
            &self.payload_package,
            &self.payload_version,
        ) {
            (None, None, None) => Ok(None),
            (Some(setup), Some(package), Some(version)) => Ok(Some(PayloadFiles {
                setup: setup.clone(),
                package: package.clone(),
                version: version.clone(),
            })),
            _ => Err(usage(
                "--payload, --payload-package and --payload-version must be given together",
            )),
        }
    }

    /// Ordinary builds accept no tester payload triple.
    #[cfg(not(feature = "tester"))]
    fn payload_files(&self) -> Result<Option<()>, CliError> {
        Ok(None)
    }
}

/// Builds one usage error.
fn usage(message: &str) -> CliError {
    CliError::Usage(message.to_owned())
}
