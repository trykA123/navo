use std::collections::BTreeMap;

use rusty_core::Location;

use super::query::Scope;
use super::source::{Group, Hit, Target};

const SCOPED_CAP: usize = 500;
const RECENT_COMMANDS: usize = 5;
const RECENT_FOLDERS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    TopHit,
    Recent,
    Group(Group),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Header(Heading),
    TopHit(Hit),
    Hit(Hit),
    ShowAll { group: Group, total: usize },
    Hint,
}

impl Row {
    #[must_use]
    pub fn is_selectable(&self) -> bool {
        !matches!(self, Self::Header(_) | Self::Hint)
    }
}

pub struct Results;

impl Results {
    #[must_use]
    pub fn merge(hits: Vec<Hit>, scope: Scope) -> Vec<Row> {
        let ordered = order(one_per_place(hits));
        match scope {
            Scope::Mixed => mixed(ordered),
            Scope::Commands | Scope::Files => scoped(ordered),
        }
    }

    #[must_use]
    pub fn recent(commands: Vec<Hit>, folders: Vec<Hit>) -> Vec<Row> {
        let hits: Vec<Row> = commands
            .into_iter()
            .take(RECENT_COMMANDS)
            .chain(folders.into_iter().take(RECENT_FOLDERS))
            .map(Row::Hit)
            .collect();
        let mut rows = Vec::with_capacity(hits.len() + 2);
        if !hits.is_empty() {
            rows.push(Row::Header(Heading::Recent));
            rows.extend(hits);
        }
        rows.push(Row::Hint);
        rows
    }
}

fn place_of(hit: &Hit) -> Option<&Location> {
    match (&hit.target, hit.source) {
        (Target::GoTo(location), "tabs") | (Target::Open { location, .. }, "recent") => {
            Some(location)
        }
        _ => None,
    }
}

fn one_per_place(mut hits: Vec<Hit>) -> Vec<Hit> {
    let same = |hits: &[Hit], a: usize, b: usize| match (place_of(&hits[a]), place_of(&hits[b])) {
        (Some(a), Some(b)) => a.same_place(b),
        _ => false,
    };
    let of = |source: &str| -> Vec<usize> {
        (0..hits.len())
            .filter(|&at| hits[at].source == source && place_of(&hits[at]).is_some())
            .collect()
    };
    let (tab_hits, recent_hits) = (of("tabs"), of("recent"));
    let mut dropped = vec![false; hits.len()];
    let mut kept: Vec<usize> = Vec::new();
    for at in tab_hits.into_iter().chain(recent_hits) {
        let first = kept.iter().copied().find(|&tab| same(&hits, tab, at));
        match first {
            Some(tab) => {
                hits[tab].score = hits[tab].score.max(hits[at].score);
                dropped[at] = true;
            }
            None if hits[at].source == "tabs" => kept.push(at),
            None => {}
        }
    }
    let mut at = 0;
    hits.retain(|_| {
        at += 1;
        !dropped[at - 1]
    });
    hits
}

fn order(mut hits: Vec<Hit>) -> Vec<Hit> {
    let mut sources: Vec<&'static str> = Vec::new();
    for hit in &hits {
        if !sources.contains(&hit.source) {
            sources.push(hit.source);
        }
    }
    let source_rank = |hit: &Hit| sources.iter().position(|id| *id == hit.source);
    let path_len = |hit: &Hit| hit.title.len() + hit.detail.len();
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| source_rank(a).cmp(&source_rank(b)))
            .then_with(|| path_len(a).cmp(&path_len(b)))
    });
    hits
}

fn mixed_cap(group: Group) -> usize {
    match group {
        Group::Files => 8,
        Group::Commands | Group::Folders | Group::Module(_) => 5,
    }
}

fn by_group(hits: Vec<Hit>) -> BTreeMap<Group, Vec<Hit>> {
    let mut groups: BTreeMap<Group, Vec<Hit>> = BTreeMap::new();
    for hit in hits {
        groups.entry(hit.group).or_default().push(hit);
    }
    groups
}

fn mixed(mut ordered: Vec<Hit>) -> Vec<Row> {
    if ordered.is_empty() {
        return Vec::new();
    }
    let top = ordered.remove(0);
    let top_group = top.group;
    let mut rows = vec![Row::Header(Heading::TopHit), Row::TopHit(top)];
    for (group, hits) in by_group(ordered) {
        let total = hits.len() + usize::from(group == top_group);
        let overflow = hits.len() > mixed_cap(group);
        rows.push(Row::Header(Heading::Group(group)));
        rows.extend(hits.into_iter().take(mixed_cap(group)).map(Row::Hit));
        if overflow {
            rows.push(Row::ShowAll { group, total });
        }
    }
    rows
}

fn scoped(ordered: Vec<Hit>) -> Vec<Row> {
    let mut rows = Vec::new();
    for (group, hits) in by_group(ordered) {
        rows.push(Row::Header(Heading::Group(group)));
        rows.extend(hits.into_iter().take(SCOPED_CAP).map(Row::Hit));
    }
    rows
}
