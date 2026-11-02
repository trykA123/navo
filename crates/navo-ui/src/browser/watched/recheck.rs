use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

pub(super) struct Recheck {
    stop: Option<Sender<()>>,
    ticker: Option<JoinHandle<()>>,
}

impl Recheck {
    pub(super) fn start(every: Duration, tick: impl Fn() -> bool + Send + 'static) -> Option<Self> {
        let (stop, stopped) = mpsc::channel::<()>();
        let ticker =
            std::thread::Builder::new()
                .name("folder-recheck".to_owned())
                .spawn(move || {
                    while stopped.recv_timeout(every) == Err(RecvTimeoutError::Timeout) && tick() {}
                })
                .ok()?;
        Some(Self {
            stop: Some(stop),
            ticker: Some(ticker),
        })
    }
}

impl Drop for Recheck {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(ticker) = self.ticker.take() {
            let _ = ticker.join();
        }
    }
}
