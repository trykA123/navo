//! Pure Navo Setup wizard transitions; the shell executes effects and owns runner sessions.

use std::path::PathBuf;

use crate::{
    mode::Mode,
    placement::browse_target,
    syntax::{Candidate, FolderProblem, parse_candidate},
};

#[cfg(test)]
#[path = "flow_tests.rs"]
mod tests;

/// An installation root, kept as a native path throughout the wizard.
pub type Root = PathBuf;

/// The thirteen pages drawn by the wizard UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Welcome,
    Licence,
    Folder,
    Shortcuts,
    Installing,
    Finish,
    Failed,
    Blocked,
    Same,
    Newer,
    Ambiguous,
    Elevated,
    NoPayload,
}

/// Only fresh folders can be edited or checked; upgrade roots stay locked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Folder {
    Checking(Candidate),
    Ready(Candidate),
    Refused {
        path: PathBuf,
        problem: FolderProblem,
    },
    Locked(Root),
}

impl Folder {
    /// Returns the selected root, including invalid text kept for editing.
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Checking(candidate) | Self::Ready(candidate) => candidate.path(),
            Self::Refused { path, .. } | Self::Locked(path) => path,
        }
    }
}

/// Fresh-install shortcut choices; an upgrade leaves existing links alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcuts {
    pub desktop: bool,
    pub start_menu: bool,
}

/// Recovery actions use the session retained by the shell, never a second fresh root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallAction {
    Run,
    RetryEmpty,
    MarkAgain,
}

/// The runner receives the discovered identity, selected root and fresh shortcut choices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub mode: Box<Mode>,
    pub root: Root,
    pub shortcuts: Shortcuts,
    pub action: InstallAction,
}

/// The only work that the pure flow asks the shell to perform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    CheckFolder(Candidate),
    Install(Request),
    RescanRunning,
    StartNavo(Root),
    Close,
}

pub use crate::flow_outcome::{Completion, Failure, RunningScan, SetupExit};

/// User actions and asynchronous results accepted by the wizard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Next,
    Back,
    Install,
    Retry,
    Rescan,
    Esc,
    Close,
    Finish,
    OpenNavo,
    AcceptLicence(bool),
    SetFolder(PathBuf),
    BrowsePicked(PathBuf),
    SetShortcuts(Shortcuts),
    SetStartNavo(bool),
    FolderChecked {
        candidate: Candidate,
        result: Result<(), FolderProblem>,
    },
    SetupExited(SetupExit),
    RunningScanned(RunningScan),
    /// The shell's worker is gone; any page that waits on it would hang.
    WorkerLost,
}

/// Owned wizard state; private fields prevent bypassing consent and child ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flow {
    page: Page,
    mode: Mode,
    folder: Folder,
    licence: bool,
    shortcuts: Shortcuts,
    start_navo: bool,
    failure: Option<Failure>,
    completion: Option<Completion>,
    scanning: bool,
    running_clear: bool,
    closed: bool,
}

impl Flow {
    /// Builds the initial page from discovery; invalid default paths stay editable.
    pub fn new(mode: Mode, default: PathBuf) -> Self {
        let (page, folder) = match &mode {
            Mode::Fresh => (Page::Welcome, checked_syntax(default)),
            Mode::Upgrade(found) => (Page::Welcome, Folder::Locked(found.root.clone())),
            Mode::Same(found) => (Page::Same, Folder::Locked(found.root.clone())),
            Mode::Newer(found) => (Page::Newer, Folder::Locked(found.root.clone())),
            Mode::Ambiguous(_) => (Page::Ambiguous, checked_syntax(default)),
        };
        Self {
            page,
            mode,
            folder,
            licence: false,
            shortcuts: Shortcuts {
                desktop: true,
                start_menu: true,
            },
            start_navo: true,
            failure: None,
            completion: None,
            scanning: false,
            running_clear: false,
            closed: false,
        }
    }

    /// Returns the current page to draw.
    pub fn page(&self) -> Page {
        self.page
    }
    /// Returns the discovered mode, including an ambiguous reason for the UI.
    pub fn mode(&self) -> &Mode {
        &self.mode
    }
    /// Returns the selected folder and its current check state.
    pub fn folder(&self) -> &Folder {
        &self.folder
    }
    /// Returns the recovery classification supplied by the runner.
    pub fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref()
    }
    /// Returns verified completion information for the Finish page.
    pub fn completion(&self) -> Option<Completion> {
        self.completion
    }
    /// Returns consent, shortcut choices and the Finish launch choice for drawing controls.
    pub fn choices(&self) -> (bool, Shortcuts, bool) {
        (self.licence, self.shortcuts, self.start_navo)
    }

    /// Reports whether a retry can request work; running scans serialize retries.
    pub fn can_retry(&self) -> bool {
        let safe = matches!(
            self.failure,
            Some(
                Failure::EmptyRoot(_) | Failure::Upgrade(_) | Failure::Unmarked | Failure::Extract
            )
        ) || (self.failure == Some(Failure::NavoStarted) && self.running_clear);
        !self.closed
            && !self.scanning
            && (self.page == Page::Blocked || (self.page == Page::Failed && safe))
    }
    /// Advances only through the consent and fresh-folder gates.
    fn next(&mut self) -> Vec<Effect> {
        match self.page {
            Page::Welcome => self.page = Page::Licence,
            Page::Licence if self.licence => {
                self.page = Page::Folder;
                if let Folder::Checking(candidate) = &self.folder {
                    return vec![Effect::CheckFolder(candidate.clone())];
                }
            }
            Page::Folder if matches!(self.folder, Folder::Ready(_)) => self.page = Page::Shortcuts,
            _ => {}
        }
        Vec::new()
    }

    /// Moves to a previous editable page while preserving any unsafe-root refusal.
    /// Leaving a failure that installed nothing clears it, and a fresh folder is checked again.
    fn back(&mut self) -> Vec<Effect> {
        self.scanning = false;
        self.running_clear = false;
        let left_failed = self.page == Page::Failed;
        self.page = match self.page {
            Page::Licence => Page::Welcome,
            Page::Folder => Page::Licence,
            Page::Shortcuts | Page::Failed => Page::Folder,
            _ => self.page,
        };
        let nothing_installed = matches!(
            self.failure,
            Some(
                Failure::EmptyRoot(_) | Failure::Upgrade(_) | Failure::Extract | Failure::Folder(_)
            )
        );
        if !(left_failed && nothing_installed) {
            return Vec::new();
        }
        self.failure = None;
        if self.mode != Mode::Fresh {
            return Vec::new();
        }
        self.folder = checked_syntax(self.folder.path().to_owned());
        match &self.folder {
            Folder::Checking(candidate) => vec![Effect::CheckFolder(candidate.clone())],
            _ => Vec::new(),
        }
    }

    /// Shows Failed on any page once the worker is gone; nothing can run any more.
    fn worker_lost(&mut self) {
        self.page = Page::Failed;
        self.failure = Some(Failure::WorkerLost);
        self.scanning = false;
        self.running_clear = false;
    }

    /// Starts one runner action; only SetupExited can release Installing afterward.
    fn install(&mut self, action: InstallAction) -> Vec<Effect> {
        self.page = Page::Installing;
        self.scanning = false;
        self.running_clear = false;
        self.failure = None;
        vec![Effect::Install(Request {
            mode: Box::new(self.mode.clone()),
            root: self.folder.path().to_owned(),
            shortcuts: self.shortcuts,
            action,
        })]
    }

    /// Refuses forged Install actions on the wrong page, unchecked roots or recovery pages.
    fn can_install(&self) -> bool {
        self.licence
            && self.failure.is_none()
            && matches!(
                (&self.mode, self.page, &self.folder),
                (Mode::Fresh, Page::Shortcuts, Folder::Ready(_))
                    | (Mode::Upgrade(_), Page::Folder, Folder::Locked(_))
            )
    }

    /// Rescans blocked upgrades; other retries reuse an empty root or only write its marker.
    fn retry(&mut self) -> Vec<Effect> {
        if !self.can_retry() {
            return Vec::new();
        }
        if self.page == Page::Blocked {
            self.scanning = true;
            return vec![Effect::RescanRunning];
        }
        let action = match self.failure {
            Some(Failure::Unmarked) => InstallAction::MarkAgain,
            Some(Failure::EmptyRoot(_)) => InstallAction::RetryEmpty,
            _ => InstallAction::Run,
        };
        self.install(action)
    }

    /// Applies a finished runner result, keeping recovery state until retry or Close.
    fn exited(&mut self, exit: SetupExit) {
        match exit {
            SetupExit::Installed(completion) => {
                self.page = Page::Finish;
                self.completion = Some(completion);
            }
            SetupExit::Failed(failure) => {
                self.page = Page::Failed;
                self.failure = Some(failure);
            }
            SetupExit::Blocked => self.page = Page::Blocked,
            SetupExit::ModeChanged(mode) => *self = Self::new(*mode, self.folder.path().to_owned()),
            SetupExit::Elevated => self.page = Page::Elevated,
            SetupExit::NoPayload => self.page = Page::NoPayload,
        }
    }

    /// Validates a fresh edit immediately and asks the shell to check its placement.
    fn edit_folder(&mut self, path: PathBuf) -> Vec<Effect> {
        if self.page != Page::Folder || self.mode != Mode::Fresh {
            return Vec::new();
        }
        if self.folder.path() != path {
            self.failure = None;
        }
        self.folder = checked_syntax(path);
        match &self.folder {
            Folder::Checking(candidate) => vec![Effect::CheckFolder(candidate.clone())],
            _ => Vec::new(),
        }
    }

    /// Drops results for other candidates and duplicate results for completed checks.
    fn folder_checked(&mut self, candidate: Candidate, result: Result<(), FolderProblem>) {
        if !matches!(&self.folder, Folder::Checking(current) if current == &candidate) {
            return;
        }
        self.folder = match result {
            Ok(()) => Folder::Ready(candidate),
            Err(problem) => Folder::Refused {
                path: candidate.path().to_owned(),
                problem,
            },
        };
    }

    /// Closes once, optionally launching Navo before releasing the shell's session.
    fn close(&mut self, start: bool) -> Vec<Effect> {
        self.closed = true;
        let mut effects = Vec::new();
        if start {
            effects.push(Effect::StartNavo(self.folder.path().to_owned()));
        }
        effects.push(Effect::Close);
        effects
    }
}

/// Parses editable path syntax; placement checking belongs to the shell's effect.
fn checked_syntax(path: PathBuf) -> Folder {
    match parse_candidate(path.as_os_str()) {
        Ok(candidate) => Folder::Checking(candidate),
        Err(problem) => Folder::Refused { path, problem },
    }
}

/// Applies one event without I/O; ignored or stale events produce no effects.
pub fn step(mut flow: Flow, event: Event) -> (Flow, Vec<Effect>) {
    if flow.closed {
        return (flow, Vec::new());
    }
    if event == Event::WorkerLost {
        flow.worker_lost();
        return (flow, Vec::new());
    }
    if flow.page == Page::Installing {
        if let Event::SetupExited(exit) = event {
            flow.exited(exit);
            if flow.failure == Some(Failure::NavoStarted) {
                flow.scanning = true;
                return (flow, vec![Effect::RescanRunning]);
            }
        }
        return (flow, Vec::new());
    }
    let effects = match event {
        Event::Next => flow.next(),
        Event::Back => flow.back(),
        Event::Close | Event::Esc => flow.close(false),
        Event::Finish if flow.page == Page::Finish => flow.close(flow.start_navo),
        Event::OpenNavo if flow.page == Page::Same => flow.close(true),
        Event::Install if flow.can_install() => flow.install(InstallAction::Run),
        Event::Retry => flow.retry(),
        Event::Rescan
            if flow.page == Page::Failed
                && flow.failure == Some(Failure::NavoStarted)
                && !flow.scanning =>
        {
            flow.scanning = true;
            flow.running_clear = false;
            vec![Effect::RescanRunning]
        }
        Event::SetFolder(path) => flow.edit_folder(path),
        Event::BrowsePicked(parent) => flow.edit_folder(browse_target(&parent)),
        event => update(&mut flow, event),
    };
    (flow, effects)
}

/// Applies selections and asynchronous check results that do not navigate directly.
fn update(flow: &mut Flow, event: Event) -> Vec<Effect> {
    match event {
        Event::AcceptLicence(accepted) if flow.page == Page::Licence => flow.licence = accepted,
        Event::SetShortcuts(choices) if flow.page == Page::Shortcuts => flow.shortcuts = choices,
        Event::SetStartNavo(start) if flow.page == Page::Finish => flow.start_navo = start,
        Event::FolderChecked { candidate, result } => flow.folder_checked(candidate, result),
        Event::RunningScanned(result) if flow.scanning => {
            flow.scanning = false;
            if result == RunningScan::Clear {
                if flow.page == Page::Failed {
                    flow.running_clear = true;
                    return Vec::new();
                }
                return flow.install(InstallAction::Run);
            }
        }
        Event::SetupExited(exit @ (SetupExit::Elevated | SetupExit::NoPayload))
            if flow.page == Page::Welcome =>
        {
            flow.exited(exit)
        }
        _ => {}
    }
    Vec::new()
}
