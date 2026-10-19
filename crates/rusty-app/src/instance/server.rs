use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use super::Error;

#[derive(Default)]
pub(super) struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    pub(super) fn start(
        &mut self,
        run: impl FnOnce(Arc<AtomicBool>) + Send + 'static,
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
                .name("ferrite-instance".into())
                .spawn(move || run(stop))?,
        );
        Ok(())
    }

    pub(super) fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
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
