//! CLI execution: payload loading, silent runner calls and stderr failure messages.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::{
    cli::{
        CliError, Command, LogPaths, PayloadSelection, Silent, hex_hash, messages::install_message,
        parse,
    },
    flow::{InstallAction, Request, Shortcuts},
    host::Host,
    mode::{Mode, mode},
    payload::Payload,
    placement,
    runner::{self, InstallError},
};

/// Runs one parsed command and returns its IW17 exit code.
pub fn run(
    args: &[OsString],
    payload: Option<Payload>,
    host: Option<&mut dyn Host>,
    logs: &dyn Fn() -> io::Result<LogPaths>,
) -> ExitCode {
    let parsed = match parse(args) {
        Ok(parsed) => parsed,
        Err(error) => return report(&error),
    };
    let files = parsed.payload_selection();
    match parsed.command {
        Command::Version => {
            println!("navo-setup {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::Hashes { setup, package } => {
            let payload = match load_payload(payload, &files) {
                Ok(payload) => payload,
                Err(error) => return report(&error),
            };
            print_hashes(&payload, setup, package);
            ExitCode::SUCCESS
        }
        Command::Silent(options) => {
            let payload = match load_payload(payload, &files) {
                Ok(payload) => payload,
                Err(error) => return report(&error),
            };
            let logs = match logs() {
                Ok(logs) => logs,
                Err(error) => {
                    return report(&CliError::Logs(format!(
                        "cannot create the installer logs: {error}"
                    )));
                }
            };
            let Some(host) = host else {
                return report(&CliError::NoHost);
            };
            silent(&options, &payload, host, &logs)
        }
    }
}

/// Loads the tester triple or falls back to the embedded payload.
fn load_payload(payload: Option<Payload>, files: &PayloadSelection) -> Result<Payload, CliError> {
    if let Some(payload) = payload {
        return Ok(payload);
    }
    #[cfg(feature = "tester")]
    if let Some(files) = &files.files {
        return Payload::from_files(&files.setup, &files.package, files.version.clone())
            .map_err(CliError::Payload);
    }
    #[cfg(not(feature = "tester"))]
    let _ = files;
    Payload::embedded().map_err(CliError::Payload)
}

/// Prints the requested hashes as lowercase hex, one per line.
fn print_hashes(payload: &Payload, setup: bool, package: bool) {
    if setup {
        println!("{}", hex_hash(&payload.setup_sha256));
    }
    if package {
        println!("{}", hex_hash(&payload.package_sha256));
    }
}

/// Compares two install roots with Windows' ordinal case-insensitive names.
#[cfg(windows)]
fn same_root(left: &Path, right: &Path) -> bool {
    placement::same_path(left, right, &crate::probe::chain::OrdinalNames)
}

/// Compares two install roots case-insensitively for the Linux CLI tests.
#[cfg(not(windows))]
fn same_root(left: &Path, right: &Path) -> bool {
    placement::same_path(left, right, &AsciiNames)
}

/// The ASCII case-insensitive name rule used off Windows.
#[cfg(not(windows))]
struct AsciiNames;

#[cfg(not(windows))]
impl placement::NameEq for AsciiNames {
    /// Compares two names without regard to ASCII case.
    fn same(&self, a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
}

/// Runs one silent install through the runner and prints failures to stderr.
fn silent(options: &Silent, payload: &Payload, host: &mut dyn Host, logs: &LogPaths) -> ExitCode {
    match host.elevated() {
        Ok(false) => {}
        _ => return fail(&CliError::Elevated, logs),
    }
    let discovery = match host.discover() {
        Ok(discovery) => discovery,
        Err(_) => return fail(&CliError::Discovery, logs),
    };
    let found = mode(&discovery, &payload.version);
    if let Mode::Upgrade(install) | Mode::Same(install) | Mode::Newer(install) = &found
        && let Some(requested) = &options.install_to
        && !same_root(requested, &install.root)
    {
        return fail(&CliError::AlreadyInstalled(install.root.clone()), logs);
    }
    let upgrade = matches!(found, Mode::Upgrade(_));
    let root = match &found {
        Mode::Upgrade(install) => install.root.clone(),
        Mode::Fresh => match options.install_to.clone() {
            Some(root) => root,
            None => match default_root() {
                Ok(root) => root,
                Err(error) => return fail(&error, logs),
            },
        },
        Mode::Same(install) | Mode::Newer(install) => install.root.clone(),
        Mode::Ambiguous(_) => options.install_to.clone().unwrap_or_default(),
    };
    if upgrade && (!options.desktop || !options.start_menu) {
        let line = "An upgrade keeps the existing shortcuts; --no-desktop and --no-start-menu have no effect.";
        eprintln!("navo-setup: {line}");
        write_log_line(&logs.wizard, &format!("navo-setup: {line}"));
    }
    let request = Request {
        mode: Box::new(found),
        root: root.clone(),
        shortcuts: Shortcuts {
            desktop: options.desktop,
            start_menu: options.start_menu,
        },
        action: InstallAction::Run,
    };
    match runner::install(request, payload, &logs.velopack, host, &mut |_| {}) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            let message = install_message(&error, &root, upgrade);
            report_install(error, host, &message, logs)
        }
    }
}

/// Appends one line to the wizard log; a write failure never stops the install.
pub(crate) fn write_log_line(path: &Path, line: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

/// Prints one install failure and closes a retained empty-root session.
fn report_install(
    error: InstallError,
    host: &mut dyn Host,
    message: &str,
    logs: &LogPaths,
) -> ExitCode {
    let code = error.exit_code();
    if let InstallError::RetryEmpty(session, _) = error {
        runner::close(session, host);
    }
    print_failure(message, logs);
    ExitCode::from(code)
}

/// Prints one CLI error after the logs exist, with the log paths, and returns its IW17 code.
fn fail(error: &CliError, logs: &LogPaths) -> ExitCode {
    print_failure(&error.message(), logs);
    ExitCode::from(error.exit_code())
}

/// Prints a failure and the log paths to stderr and appends the same lines to the wizard log.
///
/// The Velopack log is named only when a Velopack process wrote it.
fn print_failure(message: &str, logs: &LogPaths) {
    let mut lines = vec![
        format!("navo-setup: {message}"),
        format!("navo-setup: log: {}", logs.wizard.display()),
    ];
    if logs.velopack.is_file() {
        lines.push(format!(
            "navo-setup: Velopack log: {}",
            logs.velopack.display()
        ));
    }
    for line in lines {
        eprintln!("{line}");
        write_log_line(&logs.wizard, &line);
    }
}

/// Resolves the default fresh root `FOLDERID_UserProgramFiles\Navo` without creating anything.
#[cfg(windows)]
pub(crate) fn default_root() -> Result<PathBuf, CliError> {
    use windows_sys::Win32::UI::Shell::FOLDERID_UserProgramFiles;
    crate::probe::chain::known_folder_unverified(&FOLDERID_UserProgramFiles)
        .map_err(|_| CliError::Folder("the default install folder is unavailable".to_owned()))?
        .map(|base| crate::default_root::under(&base))
        .ok_or_else(|| CliError::Folder("the default install folder is unavailable".to_owned()))
}

/// Refuses the default root off Windows, where silent mode never runs.
#[cfg(not(windows))]
pub(crate) fn default_root() -> Result<PathBuf, CliError> {
    Err(CliError::NoHost)
}

/// Prints one CLI error and returns its IW17 code.
fn report(error: &CliError) -> ExitCode {
    eprintln!("navo-setup: {}", error.message());
    ExitCode::from(error.exit_code())
}
