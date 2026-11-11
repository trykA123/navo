//! Maps the pure setup `Flow` onto navo-ui's `SetupView`, the plain data the pages draw.

use std::path::PathBuf;

use navo_ui::setup::{SetupAction, SetupPage, SetupProgress, SetupView};

use crate::cli::messages::{failure_message, unsafe_name};
use crate::flow::{Failure, Flow, Folder, Page};
use crate::mode::{Mode, Why};
use crate::runner::Progress;

/// Facts fixed when the window opens: the payload version, its space need and the log files.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub version: String,
    /// Bytes Velopack needs on the target drive.
    pub space_needed: Option<u64>,
    /// The wizard's own log for this run; the window writes a failure's problem there.
    pub wizard_log: Option<PathBuf>,
    /// Further logs to name, such as Velopack's.
    pub log_paths: Vec<PathBuf>,
}

/// Values that change while the window is open but live outside the flow.
#[derive(Clone, Copy, Debug, Default)]
pub struct Live {
    pub progress: Option<Progress>,
    /// Free bytes on the drive of the folder last checked.
    pub space_free: Option<u64>,
    pub show_keys: bool,
}

/// Builds the view for the current flow state; it is drawn as is, with no further gating.
pub fn view(flow: &Flow, facts: &Facts, live: &Live) -> SetupView {
    let (licence, shortcuts, start_navo) = flow.choices();
    let folder = flow.folder();
    SetupView {
        page: page(flow.page()),
        can_next: can_next(flow),
        can_back: matches!(
            flow.page(),
            Page::Licence | Page::Folder | Page::Shortcuts | Page::Failed
        ),
        primary: primary(flow),
        folder_text: folder.path().display().to_string(),
        problem: problem(flow),
        progress: live.progress.map(|p| SetupProgress {
            elapsed: p.elapsed,
            still_working: p.still_working,
        }),
        version: facts.version.clone(),
        installed_version: installed(flow.mode()),
        folder_locked: matches!(folder, Folder::Locked(_)),
        licence_accepted: licence,
        desktop: shortcuts.desktop,
        start_menu: shortcuts.start_menu,
        start_navo,
        can_retry: flow.can_retry(),
        notice: notice(flow),
        leftover_root: matches!(flow.failure(), Some(Failure::PartialRoot(_)))
            .then(|| folder.path().to_owned()),
        log_paths: log_paths(facts),
        details: details(flow, facts),
        space_needed: facts.space_needed,
        space_free: live.space_free,
        show_keys: live.show_keys,
    }
}

/// Mirrors one flow page as the UI's page.
pub fn page(page: Page) -> SetupPage {
    match page {
        Page::Welcome => SetupPage::Welcome,
        Page::Licence => SetupPage::Licence,
        Page::Folder => SetupPage::Folder,
        Page::Shortcuts => SetupPage::Shortcuts,
        Page::Installing => SetupPage::Installing,
        Page::Finish => SetupPage::Finish,
        Page::Failed => SetupPage::Failed,
        Page::Blocked => SetupPage::Blocked,
        Page::Same => SetupPage::Same,
        Page::Newer => SetupPage::Newer,
        Page::Ambiguous => SetupPage::Ambiguous,
        Page::Elevated => SetupPage::Elevated,
        Page::NoPayload => SetupPage::NoPayload,
    }
}

/// Next opens Licence, then Folder after consent, then Shortcuts once a fresh folder passed.
fn can_next(flow: &Flow) -> bool {
    match flow.page() {
        Page::Welcome => true,
        Page::Licence => flow.choices().0,
        Page::Folder => matches!(flow.folder(), Folder::Ready(_)),
        _ => false,
    }
}

/// The footer's main button: Next, Install (as Primary), Finish, Open Navo, or Close.
fn primary(flow: &Flow) -> Option<SetupAction> {
    let upgrade = matches!(flow.mode(), Mode::Upgrade(_));
    match flow.page() {
        Page::Welcome | Page::Licence => Some(SetupAction::Next),
        Page::Folder if !upgrade => Some(SetupAction::Next),
        Page::Folder | Page::Shortcuts | Page::Finish | Page::Same => Some(SetupAction::Primary),
        Page::Installing => None,
        _ => Some(SetupAction::Close),
    }
}

/// The installed version shown on Welcome (upgrade), Same and Newer.
fn installed(mode: &Mode) -> Option<String> {
    match mode {
        Mode::Upgrade(found) | Mode::Same(found) | Mode::Newer(found) => found
            .sq_version
            .as_ref()
            .ok()
            .map(|sq| sq.version.to_string()),
        Mode::Fresh | Mode::Ambiguous(_) => None,
    }
}

/// The refused folder's reason and fix, or the runner failure's message on Failed.
fn problem(flow: &Flow) -> Option<String> {
    match (flow.page(), flow.folder(), flow.failure()) {
        (Page::Folder, Folder::Refused { problem, .. }, _) => {
            Some(format!("{} {}", problem.message(), problem.fix()))
        }
        (Page::Failed, folder, Some(failure)) => {
            let upgrade = matches!(flow.mode(), Mode::Upgrade(_));
            Some(failure_message(failure, folder.path(), upgrade))
        }
        _ => None,
    }
}

/// Completion and recovery notices, and the unsafe-name text that replaces "uninstall first".
fn notice(flow: &Flow) -> Option<String> {
    match (flow.page(), flow.mode()) {
        (Page::Finish, _) => flow.completion()?.notice().map(str::to_owned),
        (Page::Failed, _) => flow.failure()?.notice().map(str::to_owned),
        // IW8: this text must never suggest uninstalling, the step the character rule guards.
        (Page::Ambiguous, Mode::Ambiguous(Why::UnsafeName { root, character })) => {
            Some(unsafe_name(root, *character))
        }
        _ => None,
    }
}

/// Lists the logs to name: the wizard's own log first, then the others such as Velopack's.
fn log_paths(facts: &Facts) -> Vec<PathBuf> {
    facts
        .wizard_log
        .iter()
        .chain(&facts.log_paths)
        .cloned()
        .collect()
}

/// The text Copy details puts on the clipboard: the problem, then one log path per line.
fn details(flow: &Flow, facts: &Facts) -> String {
    let mut lines: Vec<String> = problem(flow).into_iter().collect();
    lines.extend(log_paths(facts).iter().map(|p| p.display().to_string()));
    lines.join("\n")
}
