pub(super) const MIN_TAB: f32 = 88.;
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

fn needed(count: usize, with_more: bool, budget: &Budget) -> f32 {
    let gaps = count.saturating_sub(1) as f32 * budget.gap;
    let more = if with_more {
        budget.gap + MORE_WIDTH
    } else {
        0.
    };
    count as f32 * MIN_TAB + gaps + more + trailing(budget)
}

fn trailing(budget: &Budget) -> f32 {
    match budget.trailing > 0. {
        true => budget.trailing + budget.gap,
        false => 0.,
    }
}

pub(super) fn plan(count: usize, active: usize, budget: &Budget) -> Plan {
    if needed(count, false, budget) <= budget.width {
        return Plan {
            shown: (0..count).collect(),
            min_tab: MIN_TAB,
        };
    }
    let fits = |k: &usize| needed(*k, true, budget) <= budget.width;
    let k = (1..count).rev().find(fits).unwrap_or(1);
    let mut shown: Vec<usize> = (0..k).collect();
    if active >= k && active < count {
        shown[k - 1] = active;
    }
    let room = budget.width - trailing(budget) - budget.gap - MORE_WIDTH;
    let per_tab = (room - (k - 1) as f32 * budget.gap) / k as f32;
    Plan {
        shown,
        min_tab: MIN_TAB.min(per_tab.max(0.)),
    }
}
