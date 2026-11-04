use std::fmt::Write;
use std::time::{Duration, Instant};

use navo_diag::ProcessMemory;

use super::answer::{SharedVolume, State, read_before};
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
        match read_before(&volume.shared, Instant::now() + READ_WAIT) {
            Some(state) => push_volume(&mut line, &state),
            None => line.push_str(" | volume busy"),
        }
    }
    line
}

fn push_volume(line: &mut String, state: &State) {
    let (entries, slots) = state.index.as_ref().map_or((0, 0), |index| {
        (index.table.len() as u64, u64::from(index.table.slots()))
    });
    let builds = &state.builds;
    let progress = state
        .status
        .progress
        .map_or_else(|| "idle".to_owned(), |percent| percent.to_string());
    let _ = write!(
        line,
        " | {} entries={entries} slots={slots} progress={progress} full_builds={} deferred={} unresolved_batches={} dropped={}",
        state.status.root,
        builds.full_builds,
        builds.deferred_rebuilds,
        builds.unresolved_batches,
        builds.dropped_changes
    );
}

#[cfg(test)]
mod tests {
    use navo_index::pipe::{VolumeBuilds, VolumeStatus};

    use super::*;

    #[test]
    fn a_heartbeat_names_the_process_memory_and_each_volume_by_letter_only() {
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
                "heartbeat pid={} working_set=73400320 private=681574400 | C: entries=0 slots=0 progress=0 full_builds=2 deferred=1 unresolved_batches=0 dropped=0",
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
        assert!(busy.ends_with("memory=unknown | volume busy"));
    }
}
