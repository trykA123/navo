use std::io::{BufReader, BufWriter};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use super::{Launcher, reaper};
use crate::protocol::{CodecError, PdfError, Request, Response, read_response, write_request};

type Reply = Result<Response, CodecError>;

const CANCEL_SLICE: Duration = Duration::from_millis(25);

pub(super) enum Prepared {
    Waiting,
    Ready,
    Failed,
}

pub(super) struct Helper {
    pub(super) child: Option<Child>,
    input: BufWriter<ChildStdin>,
    replies: Receiver<Reply>,
    pub(super) shutdowns: Arc<AtomicU64>,
    born: u64,
    _confinement: Option<Box<dyn Send>>,
}

impl Helper {
    pub(super) fn spawn(launcher: &Launcher, born: u64) -> Result<Self, PdfError> {
        launcher.check_generation(born)?;
        let mut child = Command::new(&launcher.program)
            .args(&launcher.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| PdfError::NoHelper(error.to_string()))?;
        let confinement = match launcher.confine.as_ref().map(|confine| confine(&child)) {
            None => None,
            Some(Ok(guard)) => Some(guard),
            Some(Err(error)) => {
                reaper::dispose(child, launcher.shutdowns.clone());
                return Err(PdfError::NoHelper(format!("cannot confine it: {error}")));
            }
        };
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            reaper::dispose(child, launcher.shutdowns.clone());
            return Err(PdfError::NoHelper("its pipes are missing".into()));
        };
        let (sender, replies) = mpsc::sync_channel(1);
        let reader = thread::Builder::new()
            .name("pdf-helper-reader".into())
            .spawn(move || forward_replies(output, &sender))
            .map_err(|error| PdfError::NoHelper(error.to_string()));
        if let Err(error) = reader {
            reaper::dispose(child, launcher.shutdowns.clone());
            return Err(error);
        }
        let helper = Self {
            child: Some(child),
            input: BufWriter::new(input),
            replies,
            shutdowns: launcher.shutdowns.clone(),
            born,
            _confinement: confinement,
        };
        launcher.check_generation(born)?;
        Ok(helper)
    }

    pub(super) fn prepare(&mut self, timeout: Duration) -> Result<(), PdfError> {
        match self.call(&Request::Prepare, timeout)? {
            Response::Closed => Ok(()),
            Response::Failed(error) => Err(error),
            other => Err(super::unexpected_reply(&other)),
        }
    }

    pub(super) fn begin_prepare(&mut self) -> Result<(), PdfError> {
        self.send(&Request::Prepare)
    }

    pub(super) fn poll_prepare(&mut self) -> Prepared {
        if self.cancelled() {
            return Prepared::Failed;
        }
        match self.replies.try_recv() {
            Ok(Ok(Response::Closed)) => Prepared::Ready,
            Err(TryRecvError::Empty) => Prepared::Waiting,
            _ => Prepared::Failed,
        }
    }

    pub(super) fn call(
        &mut self,
        request: &Request,
        timeout: Duration,
    ) -> Result<Response, PdfError> {
        self.send(request)?;
        let deadline = Instant::now() + timeout;
        loop {
            if self.cancelled() {
                return Err(PdfError::HelperDied);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match self.replies.recv_timeout(left.min(CANCEL_SLICE)) {
                Ok(Ok(response)) if !self.cancelled() => return Ok(response),
                Ok(Ok(_)) => return Err(PdfError::HelperDied),
                Ok(Err(CodecError::Io(_) | CodecError::Truncated))
                | Err(RecvTimeoutError::Disconnected) => return Err(PdfError::HelperDied),
                Ok(Err(error)) => return Err(PdfError::Protocol(error.to_string())),
                Err(RecvTimeoutError::Timeout) if left > CANCEL_SLICE => {}
                Err(RecvTimeoutError::Timeout) => return Err(PdfError::Timeout),
            }
        }
    }

    pub(super) fn cancelled(&self) -> bool {
        self.shutdowns.load(Ordering::SeqCst) != self.born
    }

    fn send(&mut self, request: &Request) -> Result<(), PdfError> {
        if self.cancelled() {
            return Err(PdfError::HelperDied);
        }
        write_request(&mut self.input, request).map_err(|error| match error {
            CodecError::Io(_) => PdfError::HelperDied,
            other => PdfError::Protocol(other.to_string()),
        })
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            reaper::dispose(child, self.shutdowns.clone());
        }
    }
}

fn forward_replies(output: ChildStdout, sender: &SyncSender<Reply>) {
    let mut output = BufReader::new(output);
    loop {
        let reply = match read_response(&mut output) {
            Ok(Some(response)) => Ok(response),
            Ok(None) => return,
            Err(error) => Err(error),
        };
        let is_error = reply.is_err();
        if sender.send(reply).is_err() || is_error {
            return;
        }
    }
}
