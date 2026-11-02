use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use super::MenuError;

pub type Reply<T> = Sender<Result<T, MenuError>>;

type Poll<T> = Box<dyn Fn(Duration) -> Option<Result<T, MenuError>> + Send>;

pub struct Pending<T> {
    poll: Poll<T>,
    _cancel_on_drop: Option<CancelOnDrop>,
}

/// Set once the caller dropped the `Pending`; the worker then skips the request.
#[derive(Clone, Default)]
pub(super) struct Cancel(Arc<AtomicBool>);

impl Cancel {
    #[cfg(any(windows, test))]
    pub(super) fn is_set(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

struct CancelOnDrop(Cancel);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        (self.0).0.store(true, Ordering::Release);
    }
}

impl<T: Send + 'static> Pending<T> {
    pub(super) fn channel() -> (Reply<T>, Self) {
        let (sender, receiver) = mpsc::channel();
        let poll = move |timeout| match receiver.recv_timeout(timeout) {
            Ok(result) => Some(result),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Err(MenuError::Stopped)),
        };
        let pending = Self {
            poll: Box::new(poll),
            _cancel_on_drop: None,
        };
        (sender, pending)
    }

    #[cfg(any(windows, test))]
    pub(super) fn cancellable() -> (Reply<T>, Cancel, Self) {
        let (reply, mut pending) = Self::channel();
        let cancel = Cancel::default();
        pending._cancel_on_drop = Some(CancelOnDrop(cancel.clone()));
        (reply, cancel, pending)
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
        Self {
            poll: Box::new(poll),
            _cancel_on_drop: None,
        }
    }

    pub fn wait(self) -> Result<T, MenuError> {
        loop {
            if let Some(result) = (self.poll)(Duration::MAX) {
                return result;
            }
        }
    }

    pub fn wait_timeout(&self, timeout: Duration) -> Option<Result<T, MenuError>> {
        (self.poll)(timeout)
    }
}

#[cfg(any(windows, test))]
pub(super) fn answer_unless_cancelled<T>(
    cancel: &Cancel,
    reply: &Reply<T>,
    build: impl FnOnce() -> Result<T, MenuError>,
) {
    if !cancel.is_set() {
        let _ = reply.send(build());
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{MenuError, Pending, answer_unless_cancelled};

    #[test]
    fn a_dropped_pending_cancels_its_request() {
        let (_reply, cancel, pending) = Pending::<u32>::cancellable();
        assert!(!cancel.is_set());
        drop(pending);
        assert!(cancel.is_set());
    }

    #[test]
    fn a_kept_pending_gets_the_answer() {
        let (reply, cancel, pending) = Pending::<u32>::cancellable();
        let built = Cell::new(0);
        answer_unless_cancelled(&cancel, &reply, || {
            built.set(built.get() + 1);
            Ok(7)
        });
        assert_eq!(pending.wait(), Ok(7));
        assert_eq!(built.get(), 1);
    }

    #[test]
    fn a_cancelled_request_is_skipped_before_it_is_built() {
        let (reply, cancel, pending) = Pending::<u32>::cancellable();
        drop(pending);
        let built = Cell::new(0);
        answer_unless_cancelled(&cancel, &reply, || {
            built.set(built.get() + 1);
            Err(MenuError::Stopped)
        });
        assert_eq!(built.get(), 0);
    }
}
