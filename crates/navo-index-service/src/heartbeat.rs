use std::fmt::Write;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use navo_diag::ProcessMemory;

use super::answer::{SharedVolume, State, read_before};
use super::answer_stats::BUSY_REPLIES;
#[cfg(windows)]
use super::service_log::Log;

#[cfg(windows)]
const INTERVAL: Duration = Duration::from_secs(60);
const READ_WAIT: Duration = Duration::from_millis(20);

/// Writes a heartbeat line every minute until `cancel`, so restarts and rebuilds show in the log.
#[cfg(windows)]
pub(super) fn run(log: &Log, volumes: &[SharedVolume], cancel: &navo_index::pipe::CancelToken) {
    let probe = navo_diag::probe();
    let mut next = Instant::now();
    while !cancel.is_cancelled() {
        if Instant::now() >= next {
            log.line(line(volumes, probe.sample().ok()));
            next = Instant::now() + INTERVAL;
        }
        std::thread::park_timeout(Duration::from_millis(500));
    }
}

pub(super) fn line(volumes: &[SharedVolume], memory: Option<ProcessMemory>) -> String {
    let now = Instant::now();
    let mut line = format!("heartbeat pid={}", std::process::id());
    let _ = match memory {
        Some(memory) => write!(
            line,
            " working_set={} private={}",
            memory.resident_bytes, memory.private_bytes
        ),
        None => write!(line, " memory=unknown"),
    };
    for volume in volumes {
        match read_before(&volume.shared, now + READ_WAIT) {
            Some(state) => push_volume(&mut line, &state, now),
            None => line.push_str(" | volume busy"),
        }
    }
    let _ = write!(line, " busy={}", BUSY_REPLIES.swap(0, Ordering::Relaxed));
    line
}

fn push_volume(line: &mut String, state: &State, now: Instant) {
    let (entries, slots) = state.index.as_ref().map_or((0, 0), |index| {
        (index.table.len() as u64, u64::from(index.table.slots()))
    });
    let builds = &state.builds;
    let follow = &state.follow;
    let progress = state
        .status
        .progress
        .map_or_else(|| "idle".to_owned(), |percent| percent.to_string());
    let index_memory_bytes: u64 = state
        .index
        .as_ref()
        .map_or(0, |idx| (idx.table.memory_bytes() + idx.ids.memory_bytes()) as u64);
    let bytes_per_entry = index_memory_bytes.checked_div(entries).unwrap_or(0);
    let since_full_build_s = follow.last_full_build.map_or_else(
        || "none".to_owned(),
        |instant| now.saturating_duration_since(instant).as_secs().to_string(),
    );
    let _ = write!(
        line,
        " | {} entries={entries} slots={slots} progress={progress} full_builds={} deferred={} unresolved_batches={} dropped={} lag_bytes={} follow_errors={} copies={} bytes_per_entry={bytes_per_entry} since_full_build_s={since_full_build_s}",
        state.status.root,
        builds.full_builds,
        builds.deferred_rebuilds,
        builds.unresolved_batches,
        builds.dropped_changes,
        follow.lag_bytes,
        follow.errors,
        follow.index_copies,
    );
}

#[cfg(test)]
mod tests {
    use navo_index::pipe::{VolumeBuilds, VolumeStatus};

    use super::super::answer::FollowStats;
    use super::*;

    #[test]
    fn busy_replies_count_since_the_last_heartbeat() {
        // nextest runs each test in its own process, so this process-global count starts at zero.
        // The reset keeps a shared `cargo test` binary from carrying another test's count in.
        super::super::answer_stats::BUSY_REPLIES.store(0, std::sync::atomic::Ordering::Relaxed);
        let volume = SharedVolume::new(State {
            status: VolumeStatus {
                root: "C:".into(),
                ..VolumeStatus::default()
            },
            ..State::default()
        });
        let writing = volume.shared.write().unwrap();
        assert!(read_before(&volume.shared, Instant::now()).is_none());
        assert!(read_before(&volume.shared, Instant::now()).is_none());
        drop(writing);
        let first = line(std::slice::from_ref(&volume), None);
        assert!(first.ends_with(" busy=2"), "{first}");
        let second = line(std::slice::from_ref(&volume), None);
        assert!(second.ends_with(" busy=0"), "{second}");
    }

    #[test]
    fn a_heartbeat_names_the_process_memory_and_each_volume_by_letter_only() {
        BUSY_REPLIES.store(0, Ordering::Relaxed);
        let volume = SharedVolume::new(State {
            status: VolumeStatus {
                root: "C:".into(),
                progress: Some(0),
                ..VolumeStatus::default()
            },
            builds: VolumeBuilds {
                full_builds: 2,
                deferred_rebuilds: 1,
                ..VolumeBuilds::default()
            },
            ..State::default()
        });
        let memory = ProcessMemory {
            resident_bytes: 70 << 20,
            private_bytes: 650 << 20,
        };
        let text = line(std::slice::from_ref(&volume), Some(memory));
        assert_eq!(
            text,
            format!(
                "heartbeat pid={} working_set=73400320 private=681574400 | C: entries=0 slots=0 progress=0 full_builds=2 deferred=1 unresolved_batches=0 dropped=0 lag_bytes=0 follow_errors=0 copies=0 bytes_per_entry=0 since_full_build_s=none busy=0",
                std::process::id()
            )
        );
        let _writing = volume.shared.write().unwrap();
        let busy = std::thread::scope(|scope| {
            scope
                .spawn(|| line(std::slice::from_ref(&volume), None))
                .join()
                .unwrap()
        });
        assert!(busy.ends_with("memory=unknown | volume busy busy=1"));
    }

    #[test]
    fn follow_stats_appear_in_the_heartbeat_line() {
        BUSY_REPLIES.store(0, Ordering::Relaxed);
        let index = navo_index::ntfs::test_support::MftFixture::new(40, 7)
            .read()
            .index;
        let entries = index.table.len();
        let memory_per_entry = (index.table.memory_bytes() + index.ids.memory_bytes()) / entries;
        let volume = SharedVolume::new(State {
            status: VolumeStatus {
                root: "D:".into(),
                ..VolumeStatus::default()
            },
            index: Some(std::sync::Arc::new(index)),
            follow: FollowStats {
                errors: 3,
                index_copies: 2,
                lag_bytes: 4096,
                last_full_build: Some(Instant::now() - Duration::from_secs(120)),
            },
            ..State::default()
        });
        let text = line(std::slice::from_ref(&volume), None);
        assert!(text.contains("lag_bytes=4096"), "{text}");
        assert!(text.contains("follow_errors=3"), "{text}");
        assert!(text.contains("copies=2"), "{text}");
        assert!(
            text.contains(&format!("bytes_per_entry={memory_per_entry}")),
            "{text}"
        );
        assert!(text.contains("since_full_build_s=12"), "{text}");
    }
}
