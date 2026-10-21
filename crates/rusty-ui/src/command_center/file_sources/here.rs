use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use gpui_kit::SharedString;
use rusty_core::{Location, ProviderPath};
use rusty_fs::{CancelToken, Provider};
use rusty_index::search::{self, TableHit, Term as IndexTerm};
use rusty_index::table::{Flags, NameTable};
use rusty_index::walker::{self, Limits, Outcome};

use super::Place;
use crate::command_center::query::Term;
use crate::command_center::rank;
use crate::command_center::source::{
    Cancel, Capped, Group, HitSink, Request, ResultSource, Scopes,
};

pub const WALK_CAP: usize = 300_000;
pub const WALK_AGE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(40);

pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

#[derive(Clone)]
pub struct HereOptions {
    pub cap: usize,
    pub max_age: Duration,
    pub now: Clock,
}

impl Default for HereOptions {
    fn default() -> Self {
        Self {
            cap: WALK_CAP,
            max_age: WALK_AGE,
            now: Arc::new(Instant::now),
        }
    }
}

#[derive(Clone)]
struct View {
    table: Arc<NameTable>,
    version: u64,
    outcome: Option<Outcome>,
}

struct Walk {
    root: Location,
    started: Instant,
    cancel: CancelToken,
    view: Mutex<View>,
    changed: Condvar,
}

impl Walk {
    fn lock(&self) -> MutexGuard<'_, View> {
        self.view.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn view(&self) -> View {
        self.lock().clone()
    }

    fn is_running(&self) -> bool {
        self.lock().outcome.is_none()
    }

    fn is_reusable(&self, root: &Location, now: Instant, max_age: Duration) -> bool {
        let outcome = self.lock().outcome;
        self.root.same_place(root)
            && !self.cancel.is_cancelled()
            && outcome != Some(Outcome::Cancelled)
            && (outcome.is_none() || now.saturating_duration_since(self.started) < max_age)
    }

    fn publish(&self, table: &NameTable, outcome: Option<Outcome>) {
        let mut view = self.lock();
        view.table = Arc::new(table.clone());
        view.version += 1;
        view.outcome = outcome;
        self.changed.notify_all();
    }

    fn finish(&self, outcome: Outcome) {
        let mut view = self.lock();
        view.version += 1;
        view.outcome = Some(outcome);
        self.changed.notify_all();
    }

    fn run(&self, provider: &dyn Provider, limits: Limits) {
        let result = walker::walk(provider, &self.root, limits, &self.cancel, |table, _| {
            self.publish(table, None);
        });
        self.finish(result.map_or(Outcome::Cancelled, |walked| walked.outcome));
    }

    fn wait_past(&self, version: u64, cancel: &Cancel) -> bool {
        let mut view = self.lock();
        while view.version == version {
            if cancel.is_cancelled() {
                return false;
            }
            view = self
                .changed
                .wait_timeout(view, POLL)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        !cancel.is_cancelled()
    }
}

struct Shared {
    provider: Arc<dyn Provider>,
    options: HereOptions,
    walk: Mutex<Option<Arc<Walk>>>,
}

#[derive(Clone)]
pub struct HereWalks(Arc<Shared>);

impl HereWalks {
    #[must_use]
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self::with_options(provider, HereOptions::default())
    }

    #[must_use]
    pub fn with_options(provider: Arc<dyn Provider>, options: HereOptions) -> Self {
        Self(Arc::new(Shared {
            provider,
            options,
            walk: Mutex::new(None),
        }))
    }

    pub fn cancel(&self) {
        let mut slot = self.slot();
        if slot.as_ref().is_some_and(|walk| walk.is_running())
            && let Some(walk) = slot.take()
        {
            walk.cancel.cancel();
        }
    }

    fn slot(&self) -> MutexGuard<'_, Option<Arc<Walk>>> {
        self.0.walk.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn current(&self, root: &Location) -> Arc<Walk> {
        let options = &self.0.options;
        let now = (options.now)();
        let mut slot = self.slot();
        if let Some(walk) = slot.as_ref()
            && walk.is_reusable(root, now, options.max_age)
        {
            return walk.clone();
        }
        if let Some(old) = slot.take() {
            old.cancel.cancel();
        }
        let walk = Arc::new(Walk {
            root: root.clone(),
            started: now,
            cancel: CancelToken::new(),
            view: Mutex::new(View {
                table: Arc::new(NameTable::new()),
                version: 0,
                outcome: None,
            }),
            changed: Condvar::new(),
        });
        self.spawn(&walk);
        *slot = Some(walk.clone());
        walk
    }

    fn spawn(&self, walk: &Arc<Walk>) {
        let (job, provider) = (walk.clone(), self.0.provider.clone());
        let limits = Limits {
            max_entries: self.0.options.cap,
            exclude: Vec::new(),
        };
        let spawned = thread::Builder::new()
            .name("here-walk".into())
            .spawn(move || job.run(provider.as_ref(), limits));
        if spawned.is_err() {
            walk.finish(Outcome::Cancelled);
        }
    }
}

struct Seen {
    generation: u64,
    version: u64,
    outcome: Option<Outcome>,
    walk: Arc<Walk>,
}

pub struct HereSource {
    walks: HereWalks,
    seen: Mutex<Option<Seen>>,
}

impl HereSource {
    #[must_use]
    pub fn new(walks: HereWalks) -> Self {
        Self {
            walks,
            seen: Mutex::new(None),
        }
    }

    fn seen(&self) -> MutexGuard<'_, Option<Seen>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn index_terms(terms: &[Term]) -> Vec<IndexTerm<'_>> {
    terms
        .iter()
        .map(|term| match term {
            Term::Name(text) => IndexTerm::Name(text),
            Term::Path(text) => IndexTerm::Path(text),
            Term::Ext(list) => IndexTerm::Ext(list),
        })
        .collect()
}

fn location_of(table: &NameTable, id: u32, root: &Location) -> Option<Location> {
    let mut names = Vec::new();
    let mut at = id;
    while let Some(parent) = table.parent(at) {
        names.push(table.name(at)?);
        at = parent;
    }
    names.iter().rev().try_fold(root.clone(), |location, name| {
        location.join(&ProviderPath::from_text(name)).ok()
    })
}

impl ResultSource for HereSource {
    fn id(&self) -> &'static str {
        "here"
    }

    fn group(&self) -> Group {
        Group::Files
    }

    fn scopes(&self) -> Scopes {
        Scopes::FILES
    }

    fn search(&self, request: &Request, sink: &HitSink, cancel: &Cancel) {
        let (Some(root), None) = (&request.here, &request.query.path) else {
            return;
        };
        let terms = index_terms(&request.query.terms);
        if terms.is_empty() {
            return;
        }
        let walk = self.walks.current(root);
        let view = walk.view();
        *self.seen() = Some(Seen {
            generation: request.generation,
            version: view.version,
            outcome: view.outcome,
            walk: walk.clone(),
        });
        let words = request
            .query
            .terms
            .iter()
            .filter_map(|term| match term {
                Term::Name(text) => Some(text.as_str()),
                Term::Path(_) | Term::Ext(_) => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        let is_cancelled = || cancel.is_cancelled();
        for found in search::search(&view.table, &terms, request.limit, &is_cancelled) {
            if let Some(hit) = hit_of(&view.table, &walk.root, &found, &words) {
                sink.push(request.generation, hit);
            }
        }
    }

    fn wait_for_more(&self, request: &Request, cancel: &Cancel) -> bool {
        let waiting = {
            let seen = self.seen();
            seen.as_ref()
                .filter(|seen| seen.generation == request.generation && seen.outcome.is_none())
                .map(|seen| (seen.walk.clone(), seen.version))
        };
        waiting.is_some_and(|(walk, version)| walk.wait_past(version, cancel))
    }

    fn capped(&self, request: &Request) -> Option<Capped> {
        let seen = self.seen();
        let seen = seen.as_ref()?;
        (seen.generation == request.generation && seen.outcome == Some(Outcome::EntryLimit)).then(
            || Capped {
                cap: self.walks.0.options.cap,
                root: SharedString::from(seen.walk.root.as_str().to_owned()),
            },
        )
    }
}

fn hit_of(
    table: &NameTable,
    root: &Location,
    found: &TableHit,
    words: &str,
) -> Option<crate::command_center::Hit> {
    let flags = table.flags(found.id)?;
    table.parent(found.id)?;
    let place = Place::new(
        location_of(table, found.id, root)?,
        flags.contains(Flags::FOLDER),
        0.0,
    );
    let score = rank::score(
        i64::from(found.score),
        0.0,
        rank::name_bonus(&place.name, words),
    );
    let target = place.open_target();
    Some(place.hit("here", (score, found.indices.clone()), target))
}
