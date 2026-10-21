use std::io;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread;

use rusty_core::Location;

use super::Root;

pub(super) struct Retirements {
    sender: mpsc::Sender<Root>,
    pending: Arc<Mutex<Vec<Location>>>,
    #[cfg(test)]
    pub worker: thread::ThreadId,
}

impl Retirements {
    pub fn new() -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel::<Root>();
        let pending = Arc::new(Mutex::new(Vec::<Location>::new()));
        let finishing = pending.clone();
        let worker = thread::Builder::new()
            .name("close-walk-index".into())
            .spawn(move || {
                for root in receiver {
                    let location = root.location.clone();
                    drop(root);
                    finishing
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .retain(|root| !root.same_place(&location));
                }
            })?;
        #[cfg(not(test))]
        drop(worker);
        Ok(Self {
            sender,
            pending,
            #[cfg(test)]
            worker: worker.thread().id(),
        })
    }

    pub fn push(&self, root: Root) {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(root.location.clone());
        if let Err(error) = self.sender.send(root) {
            tracing::warn!("walk index retirement worker stopped");
            drop(error.0);
        }
    }

    pub fn pending(&self, location: &Location) -> bool {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|root| root.same_place(location))
    }
}
