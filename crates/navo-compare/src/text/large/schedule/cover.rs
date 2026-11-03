use crate::text::large::Covered;
use crate::text::large::budget::{Charge, Kind, Out, Paged, PagedState, Part, StepInfo};

pub(super) fn cover(
    out: Out,
    left: std::ops::Range<u64>,
    right: std::ops::Range<u64>,
) -> (Covered, Vec<StepInfo>, Vec<Part>, Charge) {
    let mut coarse = Vec::new();
    let mut paged = Vec::new();
    for (index, part) in out.parts.iter().enumerate() {
        if part.kind == Kind::Detail {
            continue;
        }
        let hunks = out.hunks_of(index);
        if let Some(hunk) = out.hunks.get(hunks.start) {
            coarse.push(hunk.left_lines.clone());
        }
        if let Kind::Paged(count) = part.kind {
            paged.push(Paged {
                left: part.pair.left.clone(),
                right: part.pair.right.clone(),
                hunks: count,
                state: PagedState::Rediffable,
            });
        }
    }
    let Out {
        hunks,
        digests,
        parts,
        steps,
        charge,
        ..
    } = out;
    let covered = Covered {
        left,
        right,
        hunks,
        digests,
        coarse,
        paged,
    };
    (covered, steps, parts, charge)
}
