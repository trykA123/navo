//! The Setup worker thread: owns the Host, the payload and any retained runner session.
//!
//! The window sends jobs over a channel; every probe and the runner run here, never on the UI thread.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};

use futures::channel::mpsc::UnboundedReceiver;

use crate::flow::{Event, Failure, InstallAction, Request, RunningScan, SetupExit};
use crate::host::Host;
use crate::mode::{Mode, Why, mode};
use crate::payload::Payload;
use crate::placement::Space;
use crate::runner::{self, InstallError, Installed, Progress, Session};
use crate::syntax::{Candidate, FolderProblem};

/// One piece of work the window asks for.
#[derive(Debug)]
pub enum Job {
    Check(Candidate),
    Install(Request),
    Rescan(PathBuf),
    Start(PathBuf),
    /// Releases a retained session; sent when the user leaves the Failed page.
    Release,
    /// Releases a retained session, then ends the thread.
    Close,
}

/// One result sent back to the window; check results carry their candidate.
#[derive(Debug)]
pub enum Reply {
    Checked {
        candidate: Candidate,
        result: Result<(), FolderProblem>,
        /// Free bytes on the candidate's drive, when the probe read them.
        free: Option<u64>,
    },
    Progress(Progress),
    Event(Event),
    /// Sent last: the session is released and the thread ends.
    Closed,
}

/// Platform steps outside the runner's Host: the placement check and the Navo launch.
pub trait Probe {
    /// Checks a fresh folder against IW7 and IW8a; also returns its drive's free bytes when known.
    fn check_folder(
        &mut self,
        candidate: &Candidate,
        space: &Space,
    ) -> (Result<(), FolderProblem>, Option<u64>);
    /// Starts the installed Navo from `root` without waiting for it.
    fn start_navo(&mut self, root: &Path) -> io::Result<()>;
}

/// The worker's owned state; `session` is a failed run's held root kept for Retry or Close.
pub struct Worker<'a> {
    pub host: &'a mut dyn Host,
    pub probe: &'a mut dyn Probe,
    /// `None` when Setup carries no payload; every install then reports NoPayload.
    pub payload: Option<&'a Payload>,
    pub velopack_log: &'a Path,
    session: Option<Session>,
    /// The held session follows a Setup.exe exit 0: Navo is installed, so release deletes nothing.
    installed: bool,
}

impl<'a> Worker<'a> {
    /// Builds a worker with no retained session.
    pub fn new(
        host: &'a mut dyn Host,
        probe: &'a mut dyn Probe,
        payload: Option<&'a Payload>,
        velopack_log: &'a Path,
    ) -> Self {
        Self {
            host,
            probe,
            payload,
            velopack_log,
            session: None,
            installed: false,
        }
    }

    /// Serves jobs until Close or a closed channel, then releases any retained session.
    pub fn serve(&mut self, jobs: &Receiver<Job>, reply: &mut dyn FnMut(Reply)) {
        let space = self.payload.map(Payload::space);
        while let Ok(job) = jobs.recv() {
            match job {
                Job::Close => break,
                Job::Release => self.release(),
                Job::Check(candidate) => {
                    let (result, free) = match &space {
                        Some(space) => self.probe.check_folder(&candidate, space),
                        None => (Err(FolderProblem::ProbeFailed), None),
                    };
                    reply(Reply::Checked {
                        candidate,
                        result,
                        free,
                    });
                }
                Job::Install(request) => {
                    let exit = self.install(request, reply);
                    reply(Reply::Event(Event::SetupExited(exit)));
                }
                Job::Rescan(root) => reply(Reply::Event(Event::RunningScanned(self.scan(&root)))),
                // Setup closes next; a failed launch leaves Navo installed and is not an error page.
                Job::Start(root) => drop(self.probe.start_navo(&root)),
            }
        }
        self.release();
        reply(Reply::Closed);
    }

    /// Runs one install action; recovery actions need the retained session and fail closed without it.
    fn install(&mut self, request: Request, reply: &mut dyn FnMut(Reply)) -> SetupExit {
        let Some(payload) = self.payload else {
            return SetupExit::NoPayload;
        };
        let mut progress = |event| reply(Reply::Progress(event));
        let result = match request.action {
            InstallAction::Run => {
                self.release();
                runner::install(
                    request,
                    payload,
                    self.velopack_log,
                    self.host,
                    &mut progress,
                )
            }
            InstallAction::RetryEmpty => match self.session.take() {
                Some(session) => runner::retry(
                    session,
                    payload,
                    self.velopack_log,
                    self.host,
                    &mut progress,
                ),
                None => return SetupExit::Failed(Failure::Changed),
            },
            InstallAction::MarkAgain => match self.session.take() {
                Some(session) => runner::mark_again(session, self.host).map(|()| Installed {
                    completion: crate::flow::Completion::Installed,
                    legacy_marker_failed: false,
                }),
                None => return SetupExit::Failed(Failure::Changed),
            },
        };
        self.outcome(result, payload)
    }

    /// Maps a runner result to the flow's exit and keeps any session the error carries.
    fn outcome(&mut self, result: Result<Installed, InstallError>, payload: &Payload) -> SetupExit {
        let error = match result {
            Ok(done) => return SetupExit::Installed(done.completion),
            Err(error) => error,
        };
        let (failure, session) = match error {
            InstallError::Elevated => return SetupExit::Elevated,
            InstallError::Blocked => return SetupExit::Blocked,
            InstallError::ModeChanged(found) => return SetupExit::ModeChanged(found),
            InstallError::AlreadyInstalled(_) => {
                return SetupExit::ModeChanged(Box::new(self.rediscover(payload)));
            }
            InstallError::NoPayload => return SetupExit::NoPayload,
            InstallError::Extract => (Failure::Extract, None),
            InstallError::WrongAction => (Failure::Changed, None),
            InstallError::Failed(failure) => (failure, None),
            InstallError::RetryEmpty(session, code) => (Failure::EmptyRoot(code), Some(session)),
            InstallError::Unmarked(session) => (Failure::Unmarked, Some(session)),
            InstallError::Remarked(session) => (Failure::Remarked, Some(session)),
            InstallError::Unlisted(session) => (Failure::Unlisted, Some(session)),
        };
        self.installed = matches!(
            failure,
            Failure::Unmarked | Failure::Remarked | Failure::Unlisted
        );
        self.session = session;
        SetupExit::Failed(failure)
    }

    /// Reads the install state again after another install appeared; errors stay Ambiguous.
    fn rediscover(&mut self, payload: &Payload) -> Mode {
        match self.host.discover() {
            Ok(discovery) => mode(&discovery, &payload.version),
            Err(_) => Mode::Ambiguous(Why::InvalidFolder),
        }
    }

    /// Scans for a running Navo; a scan error fails closed as Failed, never Clear.
    fn scan(&mut self, root: &Path) -> RunningScan {
        match self.host.running(root) {
            Ok(false) => RunningScan::Clear,
            Ok(true) => RunningScan::Running,
            Err(_) => RunningScan::Failed,
        }
    }

    /// Closes a retained session; only a session from before the child's exit 0 removes its empty created folders.
    fn release(&mut self) {
        let installed = std::mem::take(&mut self.installed);
        match self.session.take() {
            Some(session) if installed => runner::close_installed(session, self.host),
            Some(session) => runner::close(session, self.host),
            None => {}
        }
    }
}

/// The Windows probe: chain, refused set and drive facts, then IW7's placement check.
#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsProbe;

#[cfg(windows)]
impl Probe for WindowsProbe {
    /// Any probe error refuses the folder as ProbeFailed (fail closed).
    fn check_folder(
        &mut self,
        candidate: &Candidate,
        space: &Space,
    ) -> (Result<(), FolderProblem>, Option<u64>) {
        use crate::probe::{chain, facts};
        let probed = chain::resolve_chain(candidate).and_then(|chain| {
            let refused = chain::refused()?;
            let facts = facts::facts(&chain)?;
            Ok((chain, refused, facts))
        });
        let Ok((chain, refused, facts)) = probed else {
            return (Err(FolderProblem::ProbeFailed), None);
        };
        let result = crate::placement::check(&chain, &refused, &facts, space, &chain::OrdinalNames);
        (result, Some(facts.free))
    }

    /// Starts `<root>\current\navo.exe` detached from the profile folder (IW12); fails without a profile.
    fn start_navo(&mut self, root: &Path) -> io::Result<()> {
        // A working folder inside `current\` would block Update.exe from renaming it later.
        let profile = std::env::var_os("USERPROFILE")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "USERPROFILE is not set"))?;
        std::process::Command::new(root.join("current").join("navo.exe"))
            .current_dir(profile)
            .spawn()
            .map(drop)
    }
}

/// The window's two ends of the worker: jobs out, replies in. A closed reply stream means the worker is gone.
pub struct Link {
    pub jobs: Sender<Job>,
    pub replies: UnboundedReceiver<Reply>,
}

/// Starts the worker thread; `make` builds its Host and Probe on that thread. Fails if the thread cannot start.
pub fn spawn<F>(payload: Option<Payload>, velopack_log: PathBuf, make: F) -> io::Result<Link>
where
    F: FnOnce() -> (Box<dyn Host>, Box<dyn Probe>) + Send + 'static,
{
    let (jobs, inbox) = std::sync::mpsc::channel();
    let (outbox, replies) = futures::channel::mpsc::unbounded();
    std::thread::Builder::new()
        .name("navo-setup-worker".into())
        .spawn(move || {
            let (mut host, mut probe) = make();
            let mut worker = Worker::new(&mut *host, &mut *probe, payload.as_ref(), &velopack_log);
            // A send fails only after the window is gone; the session is still released.
            worker.serve(&inbox, &mut |reply| drop(outbox.unbounded_send(reply)));
        })?;
    Ok(Link { jobs, replies })
}
