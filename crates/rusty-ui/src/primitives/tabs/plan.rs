pub(super) const MIN_TAB: f32 = 88.;
pub(super) const MIN_PINNED_TAB: f32 = 48.;
pub(super) const MORE_WIDTH: f32 = 48.;

pub(super) struct Budget {
    pub width: f32,
    pub trailing: f32,
    pub gap: f32,
}

pub(super) struct Plan {
    pub shown: Vec<usize>,
    pub min_tab: f32,
}

fn needed(shown: &[usize], pinned: &[bool], with_more: bool, budget: &Budget) -> f32 {
    let widths: f32 = shown
        .iter()
        .map(|&i| if pinned[i] { MIN_PINNED_TAB } else { MIN_TAB })
        .sum();
    let divider = shown.iter().any(|&i| pinned[i]) && shown.iter().any(|&i| !pinned[i]);
    let gaps = shown.len().saturating_sub(1) as f32 * budget.gap;
    let more = if with_more {
        budget.gap + MORE_WIDTH
    } else {
        0.
    };
    widths + gaps + more + trailing(budget) + if divider { 1. + budget.gap } else { 0. }
}

fn trailing(budget: &Budget) -> f32 {
    if budget.trailing > 0. {
        budget.trailing + budget.gap
    } else {
        0.
    }
}

pub(super) fn plan(pinned: &[bool], active: usize, budget: &Budget) -> Plan {
    let count = pinned.len();
    let all: Vec<_> = (0..count).collect();
    if needed(&all, pinned, false, budget) <= budget.width {
        return Plan {
            shown: all,
            min_tab: MIN_TAB,
        };
    }
    let shown_for = |k: usize| {
        let mut shown: Vec<_> = (0..k).collect();
        if active >= k && active < count {
            shown[k - 1] = active;
        }
        shown
    };
    let shown = (1..count)
        .rev()
        .map(shown_for)
        .find(|shown| needed(shown, pinned, true, budget) <= budget.width)
        .unwrap_or_else(|| shown_for(1));
    let room = budget.width - trailing(budget) - budget.gap - MORE_WIDTH;
    Plan {
        shown,
        min_tab: MIN_TAB.min(room.max(0.)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_tabs_fit_using_their_compact_minimum() {
        let budget = Budget {
            width: 240.,
            trailing: 0.,
            gap: 6.,
        };
        let result = plan(&[true, true, false], 2, &budget);
        assert_eq!(result.shown, [0, 1, 2]);
    }
}
