use std::time::{Duration, Instant};

use rusty_diag::components::ComponentUse;
use rusty_index::pipe::{Client, MemoryReport};

const WAIT: Duration = Duration::from_millis(50);

pub(super) fn usage() -> Vec<ComponentUse> {
    Client::memory(Instant::now() + WAIT)
        .map(|report| components(&report))
        .unwrap_or_default()
}

fn components(report: &MemoryReport) -> Vec<ComponentUse> {
    report
        .known()
        .map(|(part, found)| ComponentUse {
            name: part.label(),
            bytes: found.bytes,
            items: found.items,
            busy: report.busy,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use rusty_index::pipe::{MemoryPart, Part};

    use super::*;

    #[test]
    fn known_parts_become_fixed_labels_and_unknown_codes_are_dropped() {
        let mut report = MemoryReport::default();
        report.add(Part::Names, 4096, 10);
        report.add(Part::Process, 1 << 20, 0);
        report.parts.push(MemoryPart {
            part: 900,
            bytes: 7,
            items: 7,
        });
        let used = components(&report);
        let names: Vec<_> = used.iter().map(|component| component.name).collect();
        assert_eq!(
            names,
            ["index service names", "index service private bytes"]
        );
        assert_eq!(
            (used[0].bytes, used[0].items, used[0].busy),
            (4096, 10, false)
        );
    }

    #[test]
    fn a_busy_report_marks_every_part_busy() {
        let mut report = MemoryReport::default();
        report.add(Part::Tree, 8, 1);
        report.busy = true;
        assert!(components(&report).iter().all(|component| component.busy));
    }

    #[cfg(not(windows))]
    #[test]
    fn no_service_adds_no_parts() {
        assert!(usage().is_empty());
    }
}
