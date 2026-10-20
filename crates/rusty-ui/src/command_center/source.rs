use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gpui_kit::SharedString;
use rusty_core::{Command, Location};

use super::query::{Query, Scope};
use crate::commands::ArgKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Commands,
    Folders,
    Files,
    Module(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Action(&'static str),
    Core(Command),
    Arg(&'static str, ArgKind),
    Open { location: Location, folder: bool },
    GoTo(Location),
    ShowAll(Group),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub source: &'static str,
    pub group: Group,
    pub title: SharedString,
    pub detail: SharedString,
    pub score: i64,
    pub indices: Vec<u32>,
    pub target: Target,
    pub shortcut: Option<SharedString>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub query: Query,
    pub generation: u64,
    pub here: Option<Location>,
    pub limit: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scopes(&'static [Scope]);

impl Scopes {
    pub const ALL: Self = Self(&[Scope::Mixed, Scope::Commands, Scope::Files]);
    pub const COMMANDS: Self = Self(&[Scope::Mixed, Scope::Commands]);
    pub const FILES: Self = Self(&[Scope::Mixed, Scope::Files]);

    #[must_use]
    pub const fn new(scopes: &'static [Scope]) -> Self {
        Self(scopes)
    }

    #[must_use]
    pub fn contains(self, scope: Scope) -> bool {
        self.0.contains(&scope)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Default)]
struct SinkState {
    generation: u64,
    hits: Vec<Hit>,
}

#[derive(Clone, Debug, Default)]
pub struct HitSink(Arc<Mutex<SinkState>>);

impl HitSink {
    #[must_use]
    pub fn new(generation: u64) -> Self {
        Self(Arc::new(Mutex::new(SinkState {
            generation,
            hits: Vec::new(),
        })))
    }

    fn state(&self) -> MutexGuard<'_, SinkState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.state().generation
    }

    pub fn begin(&self, generation: u64) {
        let mut state = self.state();
        if generation > state.generation {
            state.generation = generation;
            state.hits.clear();
        }
    }

    pub fn push(&self, generation: u64, hit: Hit) -> bool {
        let mut state = self.state();
        let current = state.generation == generation;
        if current {
            state.hits.push(hit);
        }
        current
    }

    #[must_use]
    pub fn drain(&self, generation: u64) -> Vec<Hit> {
        let mut state = self.state();
        if state.generation == generation {
            std::mem::take(&mut state.hits)
        } else {
            Vec::new()
        }
    }
}

pub trait ResultSource: Send + Sync {
    fn id(&self) -> &'static str;
    fn group(&self) -> Group;
    fn scopes(&self) -> Scopes;
    fn search(&self, request: &Request, sink: &HitSink, cancel: &Cancel);
}
