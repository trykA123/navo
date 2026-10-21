use std::collections::BTreeMap;

use gpui_kit::SharedString;
use rusty_core::Location;

use super::query::Scope;
use super::source::{Capped, Group, Hit, Offline, Target};

const SCOPED_CAP: usize = 500;
const RECENT_COMMANDS: usize = 5;
const RECENT_FOLDERS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Heading {
    TopHit,
    Recent,
    Group(Group),
    FilesCapped { cap: usize, root: SharedString },
    Offline(SharedString),
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
    pub fn with_cap(mut rows: Vec<Row>, capped: Option<Capped>) -> Vec<Row> {
        let files = Row::Header(Heading::Group(Group::Files));
        if let Some(Capped { cap, root }) = capped
            && let Some(header) = rows.iter_mut().find(|row| **row == files)
        {
            *header = Row::Header(Heading::FilesCapped { cap, root });
        }
        rows
    }

    #[must_use]
    pub fn with_offline(mut rows: Vec<Row>, offline: Option<Offline>) -> Vec<Row> {
        if let Some(Offline { path }) = offline {
            rows.insert(0, Row::Header(Heading::Offline(path)));
        }
        rows
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
        (Target::GoTo(location), "tabs" | "paths")
        | (Target::Open { location, .. }, "recent" | "paths" | "here") => Some(location),
        _ => None,
    }
}

fn one_per_place(mut hits: Vec<Hit>) -> Vec<Hit> {
    let of = |hits: &[Hit], source: &str| -> Vec<usize> {
        (0..hits.len())
            .filter(|&at| hits[at].source == source && place_of(&hits[at]).is_some())
            .collect()
    };
    let mut dropped = vec![false; hits.len()];
    let mut absorb = |hits: &mut [Hit], kept: &[usize], at: usize| {
        let winner = kept.iter().copied().find(|&win| {
            matches!((place_of(&hits[win]), place_of(&hits[at])), (Some(a), Some(b)) if a.same_place(b))
        });
        if let Some(win) = winner {
            hits[win].score = hits[win].score.max(hits[at].score);
            dropped[at] = true;
        }
        winner.is_none()
    };
    let mut kept: Vec<usize> = Vec::new();
    for at in of(&hits, "tabs") {
        if absorb(&mut hits, &kept, at) {
            kept.push(at);
        }
    }
    let recents: Vec<usize> = of(&hits, "recent")
        .into_iter()
        .filter(|&at| absorb(&mut hits, &kept, at))
        .collect();
    kept.extend(recents);
    let paths: Vec<usize> = of(&hits, "paths")
        .into_iter()
        .filter(|&at| absorb(&mut hits, &kept, at))
        .collect();
    kept.extend(paths);
    for at in of(&hits, "here") {
        absorb(&mut hits, &kept, at);
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
