use std::io::{BufReader, BufWriter};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::thread;
use std::time::Duration;

use super::Launcher;
use crate::protocol::{CodecError, PdfError, Request, Response, read_response, write_request};

type Reply = Result<Response, CodecError>;

pub(super) struct Helper {
    child: Child,
    input: BufWriter<ChildStdin>,
    replies: Receiver<Reply>,
    _confinement: Option<Box<dyn Send>>,
}

impl Helper {
    pub(super) fn spawn(launcher: &Launcher) -> Result<Self, PdfError> {
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
                stop(&mut child);
                return Err(PdfError::NoHelper(format!("cannot confine it: {error}")));
            }
        };
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            stop(&mut child);
            return Err(PdfError::NoHelper("its pipes are missing".into()));
        };
        let (sender, replies) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("pdf-helper-reader".into())
            .spawn(move || forward_replies(output, &sender))
            .map_err(|error| {
                stop(&mut child);
                PdfError::NoHelper(error.to_string())
            })?;
        Ok(Self {
            child,
            input: BufWriter::new(input),
            replies,
            _confinement: confinement,
        })
    }

    pub(super) fn call(
        &mut self,
        request: &Request,
        timeout: Duration,
    ) -> Result<Response, PdfError> {
        write_request(&mut self.input, request).map_err(|error| match error {
            CodecError::Io(_) => PdfError::HelperDied,
            other => PdfError::Protocol(other.to_string()),
        })?;
        match self.replies.recv_timeout(timeout) {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(CodecError::Io(_) | CodecError::Truncated))
            | Err(RecvTimeoutError::Disconnected) => Err(PdfError::HelperDied),
            Ok(Err(error)) => Err(PdfError::Protocol(error.to_string())),
            Err(RecvTimeoutError::Timeout) => Err(PdfError::Timeout),
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        stop(&mut self.child);
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

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
