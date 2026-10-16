use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use super::MenuError;

pub type Reply<T> = Sender<Result<T, MenuError>>;

type Poll<T> = Box<dyn Fn(Duration) -> Option<Result<T, MenuError>> + Send>;

pub struct Pending<T>(Poll<T>);

impl<T: Send + 'static> Pending<T> {
    pub(super) fn channel() -> (Reply<T>, Self) {
        let (sender, receiver) = mpsc::channel();
        let poll = move |timeout| match receiver.recv_timeout(timeout) {
            Ok(result) => Some(result),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Err(MenuError::Stopped)),
        };
        (sender, Self(Box::new(poll)))
    }

    pub(super) fn ready(result: Result<T, MenuError>) -> Self {
        let (sender, pending) = Self::channel();
        let _ = sender.send(result);
        pending
    }
}

impl<T> Pending<T> {
    #[cfg(any(windows, test))]
    pub(super) fn polled(
        poll: impl Fn(Duration) -> Option<Result<T, MenuError>> + Send + 'static,
    ) -> Self {
        Self(Box::new(poll))
    }

    pub fn wait(self) -> Result<T, MenuError> {
        loop {
            if let Some(result) = (self.0)(Duration::MAX) {
                return result;
            }
        }
    }

    pub fn wait_timeout(&self, timeout: Duration) -> Option<Result<T, MenuError>> {
        (self.0)(timeout)
    }
}
