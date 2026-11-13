//! The session thread: owns the engine session and forwards its batches to the UI.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use navo_compare::model::{Generation, HashCache, ReadHint, Reason, SideSource};
use navo_compare::session::{RowBatch, Session, SessionOptions, StopBound, Totals};
use navo_core::Location;

use super::git_side::GitSide;
use super::local::LocalSide;
use super::rules;
use super::settings::{CompareSettings, engine_for, network_cap};

const POLL: Duration = Duration::from_millis(10);

/// One side of a compare.
pub enum CompareTarget {
    Local(Location),
    Git { repo: PathBuf, rev: String },
    GitRawOdb { repo: PathBuf, rev: String },
}

pub struct CompareStart {
    pub left: CompareTarget,
    pub right: CompareTarget,
    pub(crate) settings: CompareSettings,
    pub cache: Arc<dyn HashCache>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareError {
    Left(Reason),
    Right(Reason),
    Rules(String),
    /// The session thread did not start.
    Spawn(String),
}

/// What the session thread sends. Per generation: `Rows` and `Totals` in order, then `Finished`.
#[derive(Debug)]
pub enum CompareEvent {
    /// Rows, progress or a problem; its totals are also sent as `Totals`.
    Rows(RowBatch),
    Totals {
        generation: Generation,
        totals: Totals,
    },
    /// The walk waits at the network limit; `continue_walk` resumes it.
    /// `entries` were listed so far (both sides together); the rows found stay valid.
    StoppedAtLimit {
        generation: Generation,
        entries: u64,
        bound: StopBound,
    },
    /// The walk left the stopped state, because of `continue_walk`.
    Continued { generation: Generation },
    /// The last batch of this generation arrived.
    Finished { generation: Generation },
    /// The compare did not start; nothing follows.
    Failed(CompareError),
}

enum Command {
    Focus(Vec<u64>),
    StopRow(u64),
    StopAll,
    Refresh,
    ContinueWalk,
    Cancel,
}

/// An open session and the Git sides it reads, which refresh must reset.
pub(super) struct Opened {
    pub session: Session,
    pub git_sides: Vec<Arc<GitSide>>,
}

/// A running compare. Methods only queue a command and never block.
/// Dropping the handle cancels the compare and joins its thread on a helper thread.
pub struct CompareHandle {
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl CompareHandle {
    pub fn start(request: CompareStart, events: Sender<CompareEvent>) -> Self {
        Self::spawn(move || open_session(request), events)
    }

    pub(super) fn spawn(
        open: impl FnOnce() -> Result<Opened, CompareError> + Send + 'static,
        events: Sender<CompareEvent>,
    ) -> Self {
        let (commands, inbox) = mpsc::channel();
        let spawn_failed = events.clone();
        let thread = thread::Builder::new()
            .name("compare-session".to_owned())
            .spawn(move || match open() {
                Ok(opened) => pump(opened, &events, &inbox),
                Err(error) => {
                    let _ = events.send(CompareEvent::Failed(error));
                }
            })
            .inspect_err(|error| {
                tracing::warn!(%error, "compare thread did not start");
                let _ =
                    spawn_failed.send(CompareEvent::Failed(CompareError::Spawn(error.to_string())));
            })
            .ok();
        Self { commands, thread }
    }

    /// Stops the compare for good; the handle receives no more events.
    pub fn cancel(&self) {
        let _ = self.commands.send(Command::Cancel);
    }

    pub fn stop_row(&self, id: u64) {
        let _ = self.commands.send(Command::StopRow(id));
    }

    pub fn stop_all(&self) {
        let _ = self.commands.send(Command::StopAll);
    }

    /// Puts these rows first in the content-check queue.
    pub fn focus(&self, row_ids: Vec<u64>) {
        let _ = self.commands.send(Command::Focus(row_ids));
    }

    /// Resumes a walk that stopped at the network limit.
    pub fn continue_walk(&self) {
        let _ = self.commands.send(Command::ContinueWalk);
    }

    /// Starts the compare again under a new generation.
    pub fn refresh(&self) {
        let _ = self.commands.send(Command::Refresh);
    }
}

impl Drop for CompareHandle {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Cancel);
        let Some(thread) = self.thread.take() else {
            return;
        };
        let joiner = thread::Builder::new()
            .name("compare-join".to_owned())
            .spawn(move || {
                if thread.join().is_err() {
                    tracing::warn!("compare thread panicked");
                }
            });
        if let Err(error) = joiner {
            tracing::warn!(%error, "compare join thread did not start");
        }
    }
}

fn open_session(request: CompareStart) -> Result<Opened, CompareError> {
    let mut git_sides = Vec::new();
    let settings = &request.settings;
    let left = open_side(request.left, settings, &mut git_sides).map_err(CompareError::Left)?;
    let right = open_side(request.right, settings, &mut git_sides).map_err(CompareError::Right)?;
    let rules = rules::build(&*left, &*right, &request.settings)
        .map_err(|error| CompareError::Rules(error.to_string()))?;
    let options = SessionOptions {
        rules: Arc::new(rules),
        cache: request.cache,
        read_hint: ReadHint::Buffered,
        check_mode: request.settings.content_check,
        network_cap: Some(network_cap()),
    };
    let session = Session::start(left, right, options);
    Ok(Opened { session, git_sides })
}

fn open_side(
    target: CompareTarget,
    settings: &CompareSettings,
    git_sides: &mut Vec<Arc<GitSide>>,
) -> Result<Arc<dyn SideSource>, Reason> {
    Ok(match target {
        CompareTarget::Local(root) => Arc::new(LocalSide::new(root)?),
        CompareTarget::Git { repo, rev } => {
            let engine = engine_for(settings, &repo);
            let side = Arc::new(GitSide::new(repo, rev, engine)?);
            git_sides.push(Arc::clone(&side));
            side
        }
        CompareTarget::GitRawOdb { repo, rev } => {
            let side = Arc::new(GitSide::new_raw_odb(repo, rev)?);
            git_sides.push(Arc::clone(&side));
            side
        }
    })
}

/// Runs until `Cancel`, a closed command channel or a closed event channel.
/// Leaving drops the session here, which joins its workers off the UI thread.
fn pump(mut opened: Opened, events: &Sender<CompareEvent>, inbox: &Receiver<Command>) {
    let mut stopped = None;
    loop {
        if !run_commands(&mut opened, inbox) {
            return;
        }
        match opened.session.batches().recv_timeout(POLL) {
            Ok(batch) => {
                if !forward_limit(events, &mut stopped, &batch) || !forward(events, batch) {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Returns false when the compare must end.
fn run_commands(opened: &mut Opened, inbox: &Receiver<Command>) -> bool {
    loop {
        match inbox.try_recv() {
            Ok(Command::Focus(ids)) => {
                opened.session.focus(&ids);
            }
            Ok(Command::StopRow(id)) => {
                opened.session.stop_row(id);
            }
            Ok(Command::StopAll) => opened.session.stop_all(),
            Ok(Command::Refresh) => {
                for side in &opened.git_sides {
                    side.refresh();
                }
                opened.session.refresh();
            }
            Ok(Command::ContinueWalk) => {
                opened.session.continue_walk();
            }
            Ok(Command::Cancel) | Err(mpsc::TryRecvError::Disconnected) => {
                opened.session.cancel();
                return false;
            }
            Err(mpsc::TryRecvError::Empty) => return true,
        }
    }
}

/// Sends `StoppedAtLimit` or `Continued` when the batch changes the stopped state.
/// `stopped` holds the generation that is stopped; a new generation starts not stopped.
/// Returns false when the receiver is gone.
fn forward_limit(
    events: &Sender<CompareEvent>,
    stopped: &mut Option<Generation>,
    batch: &RowBatch,
) -> bool {
    let generation = batch.generation;
    let was_stopped = *stopped == Some(generation);
    let event = match batch.limit {
        Some(limit) if !was_stopped => CompareEvent::StoppedAtLimit {
            generation,
            entries: limit.entries,
            bound: limit.bound,
        },
        None if was_stopped => CompareEvent::Continued { generation },
        _ => {
            if batch.limit.is_none() {
                *stopped = None;
            }
            return true;
        }
    };
    *stopped = batch.limit.map(|_| generation);
    events.send(event).is_ok()
}

/// Returns false when the receiver is gone.
fn forward(events: &Sender<CompareEvent>, batch: RowBatch) -> bool {
    let (generation, totals, is_final) = (batch.generation, batch.totals, batch.is_final);
    let has_content =
        !batch.rows.is_empty() || !batch.progress.is_empty() || batch.problem.is_some();
    if has_content && events.send(CompareEvent::Rows(batch)).is_err() {
        return false;
    }
    if events
        .send(CompareEvent::Totals { generation, totals })
        .is_err()
    {
        return false;
    }
    !is_final || events.send(CompareEvent::Finished { generation }).is_ok()
}
