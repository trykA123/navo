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
    service: Option<Vec<VolumeCoverage>>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct VolumeCoverage {
    root: String,
    healthy: bool,
    usable: bool,
}

fn coverage(status: &Status) -> Vec<VolumeCoverage> {
    let mut volumes: Vec<_> = status
        .volumes
        .iter()
        .map(|volume| VolumeCoverage {
            root: volume
                .root
                .trim_end_matches(['\\', '/'])
                .to_ascii_uppercase(),
            healthy: volume.errors.is_empty(),
            usable: volume.entries > 0 || volume.progress.is_some(),
        })
        .collect();
    volumes.sort();
    volumes
}

impl Configured {
    pub fn service(&mut self, reply: Option<&Status>, now: Instant) -> Option<Status> {
        self.service_with(reply, now, || rusty_index::pipe::Client::connect().is_ok())
    }

    fn service_with(
        &mut self,
        reply: Option<&Status>,
        now: Instant,
        connected: impl FnOnce() -> bool,
    ) -> Option<Status> {
        if let Some(status) = reply {
            self.service = Some((now, status.clone()));
            return Some(status.clone());
        }
        match &self.service {
            Some((seen, status)) if now.duration_since(*seen) < SERVICE_GRACE && connected() => {
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
                && settled.service == service.map(coverage)
                && same_options(&settled.options, options)
        })
    }

    pub fn settle(&mut self, options: &IndexOptions, service: Option<&Status>, now: Instant) {
        self.settled = Some(Settled {
            at: now,
            options: options.clone(),
            service: service.map(coverage),
        });
    }

    pub fn query_reply(&mut self, reply: Option<&Status>, now: Instant) -> Option<Status> {
        if reply.is_some() {
            return self.service(reply, now);
        }
        if self.service.is_some() {
            self.reset();
        }
        None
    }

    pub fn reset(&mut self) {
        self.service = None;
        self.settled = None;
    }
}

pub(super) fn reset_after_actions(
    service: &mut rusty_ui::window_view::ServiceHooks,
    reset: impl Fn() + Send + Sync + 'static,
) {
    let reset: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(reset);
    for action in [&mut service.install, &mut service.remove] {
        let original = action.clone();
        let reset = reset.clone();
        *action = std::sync::Arc::new(move || {
            original();
            reset();
        });
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
        let kept = configured.service_with(None, start + Duration::from_secs(59), || true);
        assert_eq!(kept, Some(status("C:")));
        assert!(
            configured
                .service_with(None, start + Duration::from_secs(61), || true)
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

#[cfg(test)]
mod review_tests {
    use super::*;
    use rusty_index::pipe::VolumeStatus;
    fn available() -> Status {
        Status {
            volumes: vec![VolumeStatus {
                root: "C:".into(),
                entries: 20,
                ..Default::default()
            }],
        }
    }
    #[test]
    fn resetting_configuration_forgets_service_coverage() {
        let mut configured = Configured::default();
        let now = Instant::now();
        configured.service(Some(&available()), now);
        configured.reset();
        assert!(configured.service_with(None, now, || true).is_none());
    }
    #[test]
    fn coverage_is_current_when_only_entries_or_progress_change() {
        let mut configured = Configured::default();
        let now = Instant::now();
        let options = IndexOptions::default();
        let mut changed = available();
        configured.settle(&options, Some(&changed), now);
        changed.volumes[0].entries += 10;
        changed.volumes[0].progress = Some(50);
        assert!(configured.is_current(&options, Some(&changed), now));
    }
}

#[cfg(test)]
mod availability_tests {
    use super::*;
    #[test]
    fn disconnected_pipe_drops_cached_coverage_immediately() {
        let mut cache = Configured::default();
        let now = Instant::now();
        cache.service(Some(&Status::default()), now);
        assert!(cache.service_with(None, now, || false).is_none());
    }
}

#[cfg(test)]
mod action_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn install_and_remove_invalidate_coverage_after_the_action() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let action = events.clone();
        let mut service = rusty_ui::window_view::ServiceHooks::none();
        service.install = Arc::new(move || action.lock().unwrap().push("install"));
        let action = events.clone();
        service.remove = Arc::new(move || action.lock().unwrap().push("remove"));
        let reset = events.clone();
        reset_after_actions(&mut service, move || reset.lock().unwrap().push("reset"));
        (service.install)();
        (service.remove)();
        assert_eq!(
            *events.lock().unwrap(),
            ["install", "reset", "remove", "reset"]
        );
    }
    #[test]
    fn a_failed_query_discards_even_recent_service_coverage() {
        let mut cache = Configured::default();
        let now = Instant::now();
        cache.service(Some(&Status::default()), now);
        assert!(cache.query_reply(None, now).is_none());
        assert!(cache.service_with(None, now, || true).is_none());
    }
}

#[cfg(test)]
mod coverage_change_tests {
    use super::*;
    use rusty_index::pipe::VolumeStatus;
    #[test]
    fn coverage_changes_invalidate_but_error_text_changes_do_not() {
        let mut cache = Configured::default();
        let now = Instant::now();
        let options = IndexOptions::default();
        let mut status = Status {
            volumes: vec![VolumeStatus {
                root: "C:".into(),
                entries: 10,
                ..Default::default()
            }],
        };
        cache.settle(&options, Some(&status), now);
        status.volumes[0].errors.push("error".into());
        assert!(!cache.is_current(&options, Some(&status), now));
        cache.settle(&options, Some(&status), now);
        status.volumes[0].errors[0] = "another error".into();
        assert!(cache.is_current(&options, Some(&status), now));
        status.volumes[0].root = "D:".into();
        assert!(!cache.is_current(&options, Some(&status), now));
    }
}
