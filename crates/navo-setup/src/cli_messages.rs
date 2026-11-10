//! IW7 and IW17 stderr text for silent-mode failures.

use std::path::Path;

use crate::{flow::Failure, mode::Mode, runner::InstallError, syntax::FolderProblem};

/// Describes one runner failure with the root and child that produced it.
pub(super) fn install_message(error: &InstallError, fallback_root: &Path, upgrade: bool) -> String {
    match error {
        InstallError::Elevated => "start Setup normally, without Run as administrator".to_owned(),
        InstallError::Blocked => "Navo is running; close Navo and run Setup again".to_owned(),
        InstallError::AlreadyInstalled(root) => {
            format!("Navo is already installed in `{}`", root.display())
        }
        InstallError::ModeChanged(found) => mode_message(found),
        InstallError::NoPayload => "this build carries no Navo package".to_owned(),
        InstallError::Extract => "Setup could not extract its payload".to_owned(),
        InstallError::WrongAction => "internal recovery action is not valid here".to_owned(),
        InstallError::Failed(failure) => failure_message(failure, fallback_root, upgrade),
        InstallError::RetryEmpty(session, code) => child_exit(
            "Setup.exe",
            *code,
            &format!(
                "the empty install folder `{}` was removed",
                session.root().display()
            ),
        ),
        InstallError::Unmarked(session) => format!(
            "Navo is installed in `{}`, but Setup could not mark the folder.",
            session.root().display()
        ),
        InstallError::Remarked(session) => format!(
            "Navo is installed in `{}`, but its folder carries another mark. Remove Navo in Settings → Apps, then run Setup again.",
            session.root().display()
        ),
        InstallError::Unlisted(session) => format!(
            "Navo is installed in `{}`, but Windows does not list it. To remove it, run `{}`, or delete the folder.",
            session.root().display(),
            session.uninstall_command()
        ),
    }
}

/// Describes a mode change in a short line.
fn mode_message(found: &Mode) -> String {
    match found {
        Mode::Same(_) | Mode::Newer(_) => "a same or newer Navo is already installed".to_owned(),
        Mode::Ambiguous(_) => {
            "repair by uninstalling Navo in Settings, then run Setup again".to_owned()
        }
        Mode::Fresh | Mode::Upgrade(_) => "the install state changed; run Setup again".to_owned(),
    }
}

/// Describes one runner failure, including the IW7 recovery text where it applies.
pub(crate) fn failure_message(failure: &Failure, root: &Path, upgrade: bool) -> String {
    match failure {
        Failure::Folder(problem) => folder_message(*problem),
        Failure::EmptyRoot(code) => child_exit(
            "Setup.exe",
            *code,
            &format!("the empty install folder `{}` was removed", root.display()),
        ),
        Failure::PartialRoot(code) => child_exit(
            "Setup.exe",
            *code,
            &format!(
                "Setup left files in `{}`. Delete this folder before you run Setup again.",
                root.display()
            ),
        ),
        Failure::Upgrade(code) => child_exit("Update.exe apply", *code, "the upgrade failed"),
        Failure::Changed => "the install folder changed during Setup".to_owned(),
        Failure::Unmarked => format!(
            "Navo is installed in `{}`, but Setup could not mark the folder.",
            root.display()
        ),
        Failure::Remarked => format!(
            "Navo is installed in `{}`, but its folder carries another mark. Remove Navo in Settings → Apps, then run Setup again.",
            root.display()
        ),
        Failure::Unlisted => format!(
            "Navo is installed in `{}`, but Windows does not list it.",
            root.display()
        ),
        Failure::NavoStarted => "Navo started during Setup".to_owned(),
        Failure::AfterExit(code) => child_exit(
            if upgrade {
                "Update.exe apply"
            } else {
                "Setup.exe"
            },
            *code,
            "the install could not be verified",
        ),
        Failure::Extract => {
            "Setup could not unpack its installer. Free some space on the system drive, then try again."
                .to_owned()
        }
        Failure::WorkerLost => "Setup stopped unexpectedly. Close Setup and run it again.".to_owned(),
    }
}

/// Names the child and its exit code in an IW17 exit-5 message.
fn child_exit(child: &str, code: Option<u32>, detail: &str) -> String {
    match code {
        Some(code) => format!("{child} exited with code {code}; {detail}"),
        None => format!("{child} failed before it exited; {detail}"),
    }
}

/// Describes one refused folder.
fn folder_message(problem: FolderProblem) -> String {
    problem.message().to_owned()
}
