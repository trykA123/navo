//! The window's pure core: turns UI actions and worker replies into flow events and outputs.
//!
//! The GPUI entity in `window.rs` only executes the outputs, so this logic is tested without a window.

use std::io;
use std::path::PathBuf;

use navo_ui::setup::{SetupAction, SetupCheck, SetupView};

use crate::flow::{self, Effect, Event, Flow, Page, SetupExit, Shortcuts};
use crate::host::Host;
use crate::mode::{Mode, Why, mode};
use crate::payload::Payload;
use crate::placement::FolderWarning;
use crate::syntax::Candidate;
use crate::window_view::{Facts, Live, view};
use crate::window_worker::{Job, Reply};

/// Builds the first page: elevation and a missing payload refuse; discovery picks the mode.
///
/// Every probe error fails closed: elevation errors count as elevated, discovery errors as Ambiguous.
pub fn initial_flow(host: &mut dyn Host, payload: Option<&Payload>, default: PathBuf) -> Flow {
    let refuse = |exit| {
        let fresh = Flow::new(Mode::Fresh, default.clone());
        flow::step(fresh, Event::SetupExited(exit)).0
    };
    if !matches!(host.elevated(), Ok(false)) {
        return refuse(SetupExit::Elevated);
    }
    let Some(payload) = payload else {
        return refuse(SetupExit::NoPayload);
    };
    let found = host
        .discover()
        .map_or(Mode::Ambiguous(Why::InvalidFolder), |discovery| {
            mode(&discovery, &payload.version)
        });
    Flow::new(found, default)
}

/// What the window must do after one input; the core performs no I/O itself.
#[derive(Debug)]
pub enum Out {
    /// Send now to the worker.
    Send(Job),
    /// Check this folder after the typing pause; a newer check replaces it.
    CheckLater(Candidate),
    /// Drop the pending check: the folder text no longer names a checkable folder.
    CancelCheck,
    /// Open the native folder picker.
    Browse,
    OpenFolder(PathBuf),
    Copy(String),
    /// Append one line to a log file; a write error never stops the wizard.
    Log {
        path: PathBuf,
        line: String,
    },
    /// The worker released everything; end the process.
    Quit,
}

/// The flow plus the window-only values that the view needs.
#[derive(Debug)]
pub struct Shell {
    flow: Flow,
    facts: Facts,
    live: Live,
    /// The current folder's non-refusing warning; a refusal clears it.
    warning: Option<FolderWarning>,
    closing: bool,
}

impl Shell {
    /// Wraps a starting flow and the facts read before the window opened.
    pub fn new(flow: Flow, facts: Facts) -> Self {
        Self {
            flow,
            facts,
            live: Live::default(),
            warning: None,
            closing: false,
        }
    }

    /// Returns the view to draw now, with the folder warning on the Folder page.
    pub fn view(&self) -> SetupView {
        let mut view = view(&self.flow, &self.facts, &self.live);
        if self.flow.page() == Page::Folder {
            view.notice = self.warning.map(|warning| warning.message().to_owned());
        }
        view
    }

    /// Returns the current flow page.
    pub fn page(&self) -> Page {
        self.flow.page()
    }

    /// Shows or hides the access-key underlines; returns whether the view changed.
    pub fn set_show_keys(&mut self, show: bool) -> bool {
        let changed = self.live.show_keys != show;
        self.live.show_keys = show;
        changed
    }

    /// Maps one page or footer action to flow events and outputs.
    pub fn act(&mut self, action: SetupAction) -> Vec<Out> {
        let event = match action {
            SetupAction::Next => Event::Next,
            SetupAction::Back => Event::Back,
            SetupAction::Primary => match self.flow.page() {
                Page::Finish => Event::Finish,
                Page::Same => Event::OpenNavo,
                Page::Folder | Page::Shortcuts => Event::Install,
                _ => Event::Next,
            },
            SetupAction::Browse => return vec![Out::Browse],
            SetupAction::Toggle(check) => self.toggle(check),
            SetupAction::Close => Event::Close,
            SetupAction::TryAgain if self.flow.can_retry() => Event::Retry,
            SetupAction::TryAgain => Event::Rescan,
            SetupAction::OpenFolder => {
                return vec![Out::OpenFolder(self.flow.folder().path().to_owned())];
            }
            SetupAction::CopyDetails => return vec![Out::Copy(self.view().details)],
        };
        self.handle(event)
    }

    /// Builds the event that flips one check box.
    fn toggle(&self, check: SetupCheck) -> Event {
        let (licence, shortcuts, start) = self.flow.choices();
        match check {
            SetupCheck::AcceptLicence => Event::AcceptLicence(!licence),
            SetupCheck::Desktop => Event::SetShortcuts(Shortcuts {
                desktop: !shortcuts.desktop,
                ..shortcuts
            }),
            SetupCheck::StartMenu => Event::SetShortcuts(Shortcuts {
                start_menu: !shortcuts.start_menu,
                ..shortcuts
            }),
            SetupCheck::StartNavo => Event::SetStartNavo(!start),
        }
    }

    /// Steps the flow once and turns its effects into outputs.
    /// Leaving Failed releases the worker's held session first; an edit that leaves nothing to check cancels the pending check.
    pub fn handle(&mut self, event: Event) -> Vec<Out> {
        let was_failed = self.flow.page() == Page::Failed;
        let edit = matches!(event, Event::SetFolder(_) | Event::BrowsePicked(_));
        let (next, effects) = flow::step(self.flow.clone(), event);
        self.flow = next;
        if self.flow.page() != Page::Installing {
            self.live.progress = None;
        }
        let mut outs = Vec::new();
        if was_failed && !matches!(self.flow.page(), Page::Failed | Page::Installing) {
            outs.push(Out::Send(Job::Release));
        }
        if edit {
            self.warning = None;
        }
        if edit && !matches!(self.flow.folder(), flow::Folder::Checking(_)) {
            self.live.space_free = None;
            outs.push(Out::CancelCheck);
        }
        if !was_failed && self.flow.page() == Page::Failed {
            outs.extend(self.failure_log());
        }
        outs.extend(effects.into_iter().map(|effect| self.effect(effect)));
        outs
    }

    /// Builds the wizard-log line for a run that just failed, so the named log holds the problem.
    fn failure_log(&self) -> Option<Out> {
        let path = self.facts.wizard_log.clone()?;
        let problem = self.view().problem?;
        Some(Out::Log {
            path,
            line: format!("navo-setup: {problem}"),
        })
    }

    /// Builds the wizard-log line for a Navo launch that failed; the wizard still closes.
    fn start_failure_log(&self, error: &io::Error) -> Option<Out> {
        let path = self.facts.wizard_log.clone()?;
        Some(Out::Log {
            path,
            line: format!("navo-setup: cannot start Navo: {error}"),
        })
    }

    /// Turns one flow effect into an output; Close waits for the worker before quitting.
    fn effect(&mut self, effect: Effect) -> Out {
        match effect {
            Effect::CheckFolder(candidate) => {
                self.live.space_free = None;
                self.warning = None;
                Out::CheckLater(candidate)
            }
            Effect::Install(request) => Out::Send(Job::Install(request)),
            Effect::RescanRunning => Out::Send(Job::Rescan(self.flow.folder().path().to_owned())),
            Effect::StartNavo(root) => Out::Send(Job::Start(root)),
            Effect::Close => {
                self.closing = true;
                Out::Send(Job::Close)
            }
        }
    }

    /// Applies one worker reply; a stale check result changes nothing.
    pub fn reply(&mut self, reply: Reply) -> Vec<Out> {
        match reply {
            Reply::Checked {
                candidate,
                result,
                warning,
                free,
            } => {
                if matches!(self.flow.folder(), flow::Folder::Checking(current) if *current == candidate)
                {
                    self.live.space_free = free;
                    self.warning = match result {
                        Ok(()) => warning,
                        Err(_) => None,
                    };
                }
                self.handle(Event::FolderChecked { candidate, result })
            }
            Reply::Progress(progress) if self.flow.page() == Page::Installing => {
                self.live.progress = Some(progress);
                Vec::new()
            }
            Reply::Progress(_) => Vec::new(),
            Reply::Event(event) => self.handle(event),
            Reply::StartFailed(error) => self.start_failure_log(&error).into_iter().collect(),
            Reply::Closed => vec![Out::Quit],
        }
    }

    /// Handles a worker that is gone: after Close it quits; otherwise a running install shows Failed.
    pub fn worker_lost(&mut self) -> Vec<Out> {
        if self.closing {
            return vec![Out::Quit];
        }
        self.handle(Event::WorkerLost);
        Vec::new()
    }

    /// Whether Close was requested and the worker has nothing left to run.
    pub fn closing(&self) -> bool {
        self.closing
    }
}
