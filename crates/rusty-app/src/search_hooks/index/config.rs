use std::time::{Duration, Instant};

use rusty_index::pipe::Status;
use rusty_ui::command_center::IndexOptions;

const SERVICE_GRACE: Duration = Duration::from_secs(60);
const REFRESH: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct Configured {
    service: Option<(Instant, Status)>,
    settled: Option<Settled>,
}

struct Settled {
    at: Instant,
    options: IndexOptions,
    service: Option<Status>,
}

impl Configured {
    pub fn service(&mut self, reply: Option<&Status>, now: Instant) -> Option<Status> {
        if let Some(status) = reply {
            self.service = Some((now, status.clone()));
            return Some(status.clone());
        }
        match &self.service {
            Some((seen, status)) if now.duration_since(*seen) < SERVICE_GRACE => {
                Some(status.clone())
            }
            _ => {
                self.service = None;
                None
            }
        }
    }

    pub fn is_current(
        &self,
        options: &IndexOptions,
        service: Option<&Status>,
        now: Instant,
    ) -> bool {
        self.settled.as_ref().is_some_and(|settled| {
            now.duration_since(settled.at) < REFRESH
                && settled.service.as_ref() == service
                && same_options(&settled.options, options)
        })
    }

    pub fn settle(&mut self, options: &IndexOptions, service: Option<Status>, now: Instant) {
        self.settled = Some(Settled {
            at: now,
            options: options.clone(),
            service,
        });
    }

    pub fn reset(&mut self) {
        self.settled = None;
    }
}

pub(super) fn same_options(old: &IndexOptions, new: &IndexOptions) -> bool {
    old.mode == new.mode
        && old.drives == new.drives
        && old.exclude == new.exclude
        && old.network == new.network
        && old.removable == new.removable
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_index::pipe::VolumeStatus;

    fn status(root: &str) -> Status {
        Status {
            volumes: vec![VolumeStatus {
                root: root.into(),
                ..Default::default()
            }],
        }
    }

    #[test]
    fn the_last_good_service_status_lasts_sixty_seconds() {
        let mut configured = Configured::default();
        let start = Instant::now();
        assert!(configured.service(Some(&status("C:")), start).is_some());
        let kept = configured.service(None, start + Duration::from_secs(59));
        assert_eq!(kept, Some(status("C:")));
        assert!(
            configured
                .service(None, start + Duration::from_secs(61))
                .is_none()
        );
    }

    #[test]
    fn settings_are_reread_only_when_options_change_or_the_cache_expires() {
        let mut configured = Configured::default();
        let now = Instant::now();
        let mut options = IndexOptions::default();
        assert!(!configured.is_current(&options, None, now));
        configured.settle(&options, None, now);
        assert!(configured.is_current(&options, None, now + Duration::from_secs(5)));
        assert!(!configured.is_current(&options, None, now + Duration::from_secs(31)));
        options.network = !options.network;
        assert!(!configured.is_current(&options, None, now));
    }
}
