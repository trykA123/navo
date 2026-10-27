use std::sync::{Arc, Mutex, PoisonError};

use rusty_ui::command_center::Indexing;

use super::Hosts;

pub(super) fn hook(hosts: Arc<Mutex<Hosts>>) -> Arc<dyn Fn() -> Option<Indexing> + Send + Sync> {
    let last_status = Mutex::new(None);
    Arc::new(move || {
        let mut last = last_status.lock().unwrap_or_else(PoisonError::into_inner);
        if let Ok(mut hosts) = hosts.try_lock() {
            *last = hosts.indexing();
        }
        last.clone()
    })
}

impl Hosts {
    fn indexing(&mut self) -> Option<Indexing> {
        for root in &mut self.roots {
            if let Some(status) = root.host.try_status() {
                root.status = status;
            }
        }
        self.roots.iter().find_map(|root| {
            let progress = root.status.progress?;
            let path = root.location.as_str();
            let trimmed = path.trim_end_matches(['\\', '/']);
            Some(Indexing {
                root: if trimmed.is_empty() { path } else { trimmed }
                    .to_owned()
                    .into(),
                count: progress.entries as u64,
                busy: false,
            })
        })
    }
}
