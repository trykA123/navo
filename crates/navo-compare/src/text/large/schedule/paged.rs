use crate::anchor::SectionPair;

use super::super::budget::{Kind, Paged, PagedState, Part, StepInfo, StepReason};
use super::super::{DiffEvent, Driver};

pub(crate) fn send(
    driver: &Driver,
    paged: &[Paged],
    first_seq: u64,
) -> Result<(), crate::model::Reason> {
    for (at, section) in paged.iter().enumerate() {
        let pair = SectionPair {
            left: section.left.clone(),
            right: section.right.clone(),
        };
        driver.send(step_event(&StepInfo::new(&pair, StepReason::Paged)))?;
        let seq = first_seq.saturating_add(u64::try_from(at).unwrap_or(u64::MAX));
        driver.send(DiffEvent::Paged {
            paged: section.clone(),
            seq,
        })?;
    }
    Ok(())
}

pub(super) fn step_event(step: &StepInfo) -> DiffEvent {
    DiffEvent::Step {
        step: step.step,
        section: step.section.clone(),
        lines: step.lines,
        reason: step.reason,
    }
}

pub(super) fn send_counts(driver: &Driver, parts: &[Part]) -> Result<(), crate::model::Reason> {
    let mut start = 0;
    for part in parts {
        let count = u64::try_from(part.end - start).unwrap_or(u64::MAX);
        start = part.end;
        if matches!(part.kind, Kind::Paged(_)) {
            continue;
        }
        let paged = Paged {
            left: part.pair.left.clone(),
            right: part.pair.right.clone(),
            hunks: match part.kind {
                Kind::Detail => count,
                Kind::Paged(hunks) => hunks,
                Kind::Coarse => 1,
            },
            state: PagedState::CountsOnly,
        };
        send(driver, std::slice::from_ref(&paged), 0)?;
    }
    Ok(())
}
