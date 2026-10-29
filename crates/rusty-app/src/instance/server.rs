use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use super::Error;

#[derive(Default)]
pub(super) struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    wake: Option<Box<dyn FnOnce() -> io::Result<()> + Send>>,
}

impl Server {
    pub(super) fn start(
        &mut self,
        run: impl FnOnce(Arc<AtomicBool>) + Send + 'static,
        wake: impl FnOnce() -> io::Result<()> + Send + 'static,
    ) -> Result<(), Error> {
        if self.thread.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "instance server already started",
            )
            .into());
        }
        let stop = Arc::clone(&self.stop);
        self.thread = Some(
            thread::Builder::new()
                .name("navo-instance".into())
                .spawn(move || run(stop))?,
        );
        self.wake = Some(Box::new(wake));
        Ok(())
    }

    pub(super) fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let woken = match self.wake.take().map(|wake| wake()) {
            Some(Err(error)) => {
                tracing::error!(%error, "cannot wake instance server");
                false
            }
            _ => true,
        };
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            if !woken {
                tracing::error!("instance server not woken; detaching its thread");
                return;
            }
            if thread.join().is_err() {
                tracing::error!("instance server thread panicked");
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(any(windows, test))]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Completion {
    Gone,
    Done(u32),
    Fatal(u32),
}

#[cfg(any(windows, test))]
pub(super) fn classify_completion(code: u32, count: u32) -> Completion {
    const ERROR_BROKEN_PIPE: u32 = 109;
    const ERROR_PIPE_NOT_CONNECTED: u32 = 233;
    const ERROR_MORE_DATA: u32 = 234;
    match code {
        ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED => Completion::Gone,
        0 | ERROR_MORE_DATA if count == 0 => Completion::Gone,
        0 | ERROR_MORE_DATA => Completion::Done(count),
        code => Completion::Fatal(code),
    }
}

#[cfg(any(windows, test))]
pub(super) fn wait_for_pending(
    wait: impl FnOnce() -> io::Result<bool>,
    cancel: impl FnOnce(),
    complete: impl FnOnce() -> io::Result<()>,
) -> io::Result<bool> {
    let ready = wait();
    if !matches!(ready, Ok(true)) {
        cancel();
    }
    let completed = complete();
    if matches!(ready, Ok(true)) {
        completed?;
    }
    ready
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn a_failed_wake_detaches_the_server_thread_instead_of_joining() {
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let mut server = Server::default();
        server
            .start(
                move |_| {
                    let _ = blocked.recv();
                },
                || Err(io::ErrorKind::Other.into()),
            )
            .unwrap();
        let started = std::time::Instant::now();
        server.stop();
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        release.send(()).unwrap();
    }

    #[test]
    fn closed_pipes_and_zero_byte_reads_end_the_connection() {
        for (code, count) in [(109, 0), (233, 0), (0, 0)] {
            assert_eq!(classify_completion(code, count), Completion::Gone);
        }
    }

    #[test]
    fn successful_and_more_data_completions_preserve_the_byte_count() {
        for code in [0, 234] {
            assert_eq!(classify_completion(code, 7), Completion::Done(7));
        }
    }

    #[test]
    fn other_completion_errors_remain_fatal() {
        for code in [5, 232, 995, 997] {
            assert_eq!(classify_completion(code, 0), Completion::Fatal(code));
        }
    }

    #[test]
    fn a_pending_connection_completes_without_cancellation() {
        let calls = RefCell::new(Vec::new());
        assert!(
            wait_for_pending(
                || Ok(true),
                || calls.borrow_mut().push("cancel"),
                || {
                    calls.borrow_mut().push("complete");
                    Ok(())
                },
            )
            .unwrap()
        );
        assert_eq!(*calls.borrow(), ["complete"]);
    }

    #[test]
    fn stopping_or_failing_a_wait_cancels_then_drains_the_connection() {
        for wait in [
            Ok(false),
            Err(io::ErrorKind::Other.into()),
            Err(io::ErrorKind::ConnectionAborted.into()),
            Err(io::ErrorKind::TimedOut.into()),
        ] {
            let calls = RefCell::new(Vec::new());
            let failed = wait.is_err();
            let result = wait_for_pending(
                || wait,
                || calls.borrow_mut().push("cancel"),
                || {
                    calls.borrow_mut().push("complete");
                    Err(io::ErrorKind::Interrupted.into())
                },
            );
            assert_eq!(*calls.borrow(), ["cancel", "complete"]);
            if failed {
                assert!(result.is_err());
            } else {
                assert!(!result.unwrap());
            }
        }
    }
}
