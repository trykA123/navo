use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rusty_automotive::map::{MapError, index_map};
use rusty_automotive::{
    Elf, ElfReader, IndexError, MapModel, Mdf, Watch, parse_mdf, sniff_elf, sniff_map, sniff_mdf,
};
use rusty_core::Location;

use crate::prelude::*;
use crate::window_view::viewer::FileHooks;
use crate::window_view::viewer::background::{Job, spawn_job};
use crate::window_view::viewer::body::Host;

mod finder;
use finder::Finder;

pub(in crate::window_view::viewer) const MDF_LIMIT: u64 = 1024 * 1024;
pub(in crate::window_view::viewer) const MAP_LIMIT: u64 = 64 * 1024 * 1024;
pub(in crate::window_view::viewer) const ELF_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view::viewer) enum Partner {
    Mdf,
    Map,
    Elf,
}

impl Partner {
    fn extensions(self) -> [&'static str; 2] {
        match self {
            Self::Mdf => ["mdf", "MDF"],
            Self::Map => ["map", "MAP"],
            Self::Elf => ["elf", "ELF"],
        }
    }

    fn limit(self) -> u64 {
        match self {
            Self::Mdf => MDF_LIMIT,
            Self::Map => MAP_LIMIT,
            Self::Elf => ELF_LIMIT,
        }
    }

    fn sniff(self, head: &[u8]) -> bool {
        match self {
            Self::Mdf => sniff_mdf(head),
            Self::Map => sniff_map(head).is_some(),
            Self::Elf => sniff_elf(head),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) enum SiblingProblem {
    TooLarge { name: String, limit: u64 },
    NotRecognised { name: String },
    Unreadable { name: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct SiblingNames {
    pub mdf: Option<String>,
    pub map: Option<String>,
    pub elf: Option<String>,
    pub only_mdf: bool,
    pub only_map: bool,
}

#[derive(Debug, Clone, Default)]
pub(in crate::window_view::viewer) struct Siblings {
    pub mdf: Option<Mdf>,
    pub map: Option<Arc<MapModel>>,
    pub elf: Option<Arc<Elf>>,
    pub names: SiblingNames,
    pub problems: Vec<SiblingProblem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct Stem {
    folder: Location,
    stem: String,
    extension: String,
    wants_elf: bool,
}

impl Stem {
    pub(in crate::window_view::viewer) fn with_elf(self) -> Self {
        Self {
            wants_elf: true,
            ..self
        }
    }
}

pub(in crate::window_view::viewer) fn stem_of(location: &Location) -> Option<Stem> {
    let folder = location.parent()?;
    let name = location.name()?.to_text();
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, extension),
        _ => (name.as_str(), ""),
    };
    Some(Stem {
        folder,
        stem: stem.to_owned(),
        extension: extension.to_owned(),
        wants_elf: false,
    })
}

pub(in crate::window_view::viewer) fn start_siblings<V: 'static>(
    host: &Host,
    stem: Option<Stem>,
    cx: &mut Context<V>,
    apply: impl Fn(&mut V, Siblings, &mut Context<V>) + 'static,
) -> Job {
    let hooks = host.hooks.clone();
    spawn_job(
        cx,
        move |cancel, send| {
            let siblings = load_siblings(&hooks, stem.as_ref(), cancel);
            if !cancel.load(Ordering::Relaxed) {
                send(siblings);
            }
        },
        apply,
    )
}

pub(in crate::window_view::viewer) fn load_siblings(
    hooks: &FileHooks,
    stem: Option<&Stem>,
    cancel: &AtomicBool,
) -> Siblings {
    let mut siblings = Siblings::default();
    let Some(stem) = stem else {
        return siblings;
    };
    let mut finder = Finder::new(hooks, stem, cancel);
    if let Some(found) = finder.find(Partner::Mdf, &mut siblings.problems) {
        siblings.mdf = Some(parse_mdf(&String::from_utf8_lossy(&found.bytes)));
        siblings.names.mdf = Some(found.name);
        siblings.names.only_mdf = found.only;
    }
    if cancel.load(Ordering::Relaxed) {
        return siblings;
    }
    if let Some(found) = finder.find(Partner::Map, &mut siblings.problems) {
        let name = found.name;
        match index_map(found.bytes.as_slice(), cancel, &mut |_| {}) {
            Ok(model) => {
                siblings.map = Some(Arc::new(model));
                siblings.names.map = Some(name);
                siblings.names.only_map = found.only;
            }
            Err(MapError::Cancelled) => {}
            Err(MapError::UnsupportedFormat) => {
                siblings
                    .problems
                    .push(SiblingProblem::NotRecognised { name });
            }
            Err(MapError::Read(_)) => siblings.problems.push(SiblingProblem::Unreadable { name }),
        }
    }
    if stem.wants_elf && !cancel.load(Ordering::Relaxed) {
        load_elf(&mut finder, cancel, &mut siblings);
    }
    siblings
}

fn load_elf(finder: &mut Finder<'_>, cancel: &AtomicBool, siblings: &mut Siblings) {
    let Some(found) = finder.find(Partner::Elf, &mut siblings.problems) else {
        return;
    };
    let name = found.name;
    let reader = ElfReader::new(Cursor::new(found.bytes));
    let mut watch = Watch {
        cancel,
        progress: &mut |_| {},
    };
    match reader.parse(&mut watch) {
        Ok(elf) => {
            siblings.elf = Some(Arc::new(elf));
            siblings.names.elf = Some(name);
        }
        Err(IndexError::Cancelled) => {}
        Err(IndexError::Read(_) | IndexError::Malformed(_)) => {
            siblings.problems.push(SiblingProblem::Unreadable { name });
        }
    }
}

#[cfg(test)]
mod tests;
