use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use super::MenuError;

pub type Reply<T> = Sender<Result<T, MenuError>>;

pub struct Pending<T>(Receiver<Result<T, MenuError>>);

impl<T> Pending<T> {
    pub(super) fn channel() -> (Reply<T>, Self) {
        let (sender, receiver) = mpsc::channel();
        (sender, Self(receiver))
    }

    pub(super) fn ready(result: Result<T, MenuError>) -> Self {
        let (sender, pending) = Self::channel();
        let _ = sender.send(result);
        pending
    }

    pub fn wait(self) -> Result<T, MenuError> {
        self.0.recv().unwrap_or(Err(MenuError::Stopped))
    }

    pub fn wait_timeout(&self, timeout: Duration) -> Option<Result<T, MenuError>> {
        match self.0.recv_timeout(timeout) {
            Ok(result) => Some(result),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Err(MenuError::Stopped)),
        }
    }
}
