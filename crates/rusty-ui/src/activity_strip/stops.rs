use crate::activity::{ActivityId, ActivityItem, ActivityKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Pause,
    Resume,
    Cancel,
    Show,
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Button(ActivityId, Act),
    Chip(ActivityId),
    Row(ActivityId),
    All,
}

impl Stop {
    #[must_use]
    pub fn item(self) -> Option<ActivityId> {
        match self {
            Self::Button(id, _) | Self::Chip(id) | Self::Row(id) => Some(id),
            Self::All => None,
        }
    }
}

#[must_use]
pub fn acts_of(item: &ActivityItem) -> Vec<Act> {
    match item.kind {
        ActivityKind::Job => {
            let pause = if item.paused { Act::Resume } else { Act::Pause };
            let mut acts = Vec::new();
            acts.extend(item.can_pause.then_some(pause));
            acts.extend(item.can_cancel.then_some(Act::Cancel));
            acts
        }
        ActivityKind::Failure => vec![Act::Show, Act::Dismiss],
        ActivityKind::Drag | ActivityKind::Done | ActivityKind::Background => Vec::new(),
    }
}

fn buttons(item: &ActivityItem) -> impl Iterator<Item = Stop> + use<> {
    let id = item.id;
    acts_of(item)
        .into_iter()
        .map(move |act| Stop::Button(id, act))
}

#[must_use]
pub fn line_stops(items: &[ActivityItem]) -> Vec<Stop> {
    let Some((primary, rest)) = items.split_first() else {
        return Vec::new();
    };
    buttons(primary)
        .chain(rest.iter().map(|item| Stop::Chip(item.id)))
        .chain((!rest.is_empty()).then_some(Stop::All))
        .collect()
}

#[must_use]
pub fn drawer_stops(items: &[ActivityItem]) -> Vec<Stop> {
    items
        .iter()
        .flat_map(|item| {
            let stops: Vec<Stop> = buttons(item).collect();
            if stops.is_empty() {
                vec![Stop::Row(item.id)]
            } else {
                stops
            }
        })
        .collect()
}

#[must_use]
pub fn settle(stops: &[Stop], current: Option<Stop>) -> Option<Stop> {
    current
        .filter(|stop| stops.contains(stop))
        .or_else(|| stops.first().copied())
}

#[must_use]
pub fn step(stops: &[Stop], current: Option<Stop>, forward: bool) -> Option<Stop> {
    let count = stops.len();
    let at = current.and_then(|stop| stops.iter().position(|each| *each == stop));
    let next = match (at, forward) {
        (Some(index), true) => (index + 1) % count.max(1),
        (Some(index), false) => (index + count - 1) % count.max(1),
        (None, _) => 0,
    };
    stops.get(next).copied()
}

#[must_use]
pub fn step_item(stops: &[Stop], current: Option<Stop>, forward: bool) -> Option<Stop> {
    let mut firsts: Vec<Stop> = Vec::new();
    for stop in stops {
        if firsts.last().and_then(|first| first.item()) != stop.item() {
            firsts.push(*stop);
        }
    }
    let item = current.and_then(Stop::item);
    let Some(at) = firsts.iter().position(|first| first.item() == item) else {
        return firsts.first().copied();
    };
    let next = if forward {
        (at + 1).min(firsts.len() - 1)
    } else {
        at.saturating_sub(1)
    };
    firsts.get(next).copied()
}
