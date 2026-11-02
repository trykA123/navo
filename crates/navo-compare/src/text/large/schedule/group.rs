use std::sync::Arc;

use crate::anchor::SectionPair;

use super::super::batch::{Batch, Sections};
use super::super::budget::{Charge, bytes_of};

/// Splits the plan into batches of about `limit` lines; each copy takes its bytes from `charge`.
pub(crate) fn group(
    sections: Vec<SectionPair>,
    limit: u64,
    terminal: bool,
    charge: &mut Charge,
) -> Vec<Batch> {
    let mut lists = Vec::new();
    let mut current = Vec::new();
    let mut size = 0_u64;
    for section in sections {
        let lines =
            (section.left.end - section.left.start).max(section.right.end - section.right.start);
        if !current.is_empty() && size.saturating_add(lines) > limit {
            lists.push(std::mem::take(&mut current));
            size = 0;
        }
        size = size.saturating_add(lines);
        current.push(section);
    }
    if !current.is_empty() {
        lists.push(current);
    }
    let count = lists.len();
    lists
        .into_iter()
        .enumerate()
        .map(|(at, mut list)| {
            list.shrink_to_fit();
            let held = charge.take(bytes_of::<SectionPair>(list.capacity()));
            Batch {
                sections: Arc::new(Sections { list, charge: held }),
                start_anchor: at > 0,
                end_anchor: at + 1 < count,
                terminal: terminal && at + 1 == count,
            }
        })
        .collect()
}
