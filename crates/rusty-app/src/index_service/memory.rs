use std::time::{Duration, Instant};

use rusty_index::pipe::{MemoryReport, Part, Reply};

use super::answer::{SharedVolume, read_before};

const READ_WAIT: Duration = Duration::from_millis(20);

pub(super) fn answer(volumes: &[SharedVolume]) -> Reply {
    let mut memory = report_before(volumes, Instant::now() + READ_WAIT);
    if let Ok(process) = crate::diag::probe().sample() {
        memory.add(Part::Process, process.private_bytes as usize, 0);
    }
    Reply {
        memory: Some(memory),
        ..Reply::default()
    }
}

fn report_before(volumes: &[SharedVolume], deadline: Instant) -> MemoryReport {
    let mut report = MemoryReport::default();
    for volume in volumes {
        match read_before(&volume.shared, deadline) {
            Some(state) => {
                if let Some(index) = &state.index {
                    report.add_index(index);
                }
            }
            None => report.busy = true,
        }
    }
    report.add_sessions();
    report
}

#[cfg(test)]
mod tests {
    use rusty_index::pipe::{CancelToken, QueryTerm, Request, VolumeStatus};

    use super::*;
    use crate::index_service::answer::State;

    fn bytes(reply: &Reply, wanted: Part) -> Option<u64> {
        let memory = reply.memory.as_ref()?;
        memory
            .known()
            .find(|(part, _)| *part == wanted)
            .map(|(_, found)| found.bytes)
    }

    fn volume() -> SharedVolume {
        SharedVolume::new(State {
            status: VolumeStatus {
                root: "C:".into(),
                ..VolumeStatus::default()
            },
            ..State::default()
        })
    }

    #[test]
    fn a_memory_request_answers_with_the_report_and_no_hits() {
        let request = Request {
            terms: vec![QueryTerm::MemoryV1],
        };
        let reply =
            crate::index_service::answer::answer(&[volume()], &request, &CancelToken::new());
        assert!(reply.hits.is_empty() && reply.totals.is_none());
        let memory = reply.memory.as_ref().unwrap();
        assert!(!memory.busy);
        assert!(bytes(&reply, Part::Sessions).is_some());
        #[cfg(target_os = "linux")]
        assert!(bytes(&reply, Part::Process).is_some_and(|private| private > 0));
    }

    #[test]
    fn a_volume_held_for_writing_marks_the_report_busy_without_waiting() {
        let volume = volume();
        let writing = volume.shared.write().unwrap();
        let started = Instant::now();
        let report = std::thread::scope(|scope| {
            scope
                .spawn(|| report_before(std::slice::from_ref(&volume), Instant::now()))
                .join()
                .unwrap()
        });
        drop(writing);
        assert!(report.busy);
        assert!(started.elapsed() < Duration::from_millis(50));
    }
}
