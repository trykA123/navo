use std::ops::ControlFlow;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gpui_kit::SharedString;
use rusty_core::{FsErrorKind, Location, typed_location};
use rusty_fs::{CancelToken, EntryKind, Provider};
use rusty_index::fuzzy::Fuzzy;
use rusty_l10n::t;

use super::Place;
use crate::command_center::flights::{Answer, Ask, Clock, Flights, Rules};
use crate::command_center::rank;
use crate::command_center::source::{
    Cancel, Group, Hit, HitSink, Offline, Request, ResultSource, Scopes, Target,
};

pub const LIST_WAIT: Duration = Duration::from_secs(2);
const GO_TO_SCORE: i64 = 1_000_000;

pub type Env = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub(crate) struct Split {
    pub(crate) parent: Location,
    pub(crate) typed: Location,
    pub(crate) filter: String,
}

#[derive(Clone)]
enum Listing {
    Folders(Vec<Location>),
    Unreachable,
    Failed,
}

#[derive(Clone)]
pub struct PathListings {
    provider: Arc<dyn Provider>,
    flights: Flights<Listing>,
}

impl PathListings {
    #[must_use]
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self::with_clock(provider, LIST_WAIT, Arc::new(Instant::now))
    }

    #[must_use]
    pub fn with_clock(provider: Arc<dyn Provider>, wait: Duration, now: Clock) -> Self {
        let flights = Flights::new(Rules {
            name: "paths-list",
            wait,
            is_down: |listing| matches!(listing, Listing::Unreachable),
            now,
        });
        Self { provider, flights }
    }

    #[must_use]
    pub fn is_unreachable(&self, location: &Location) -> bool {
        self.flights.is_unreachable(location)
    }

    fn list(&self, parent: &Location, since: Instant, cancel: &Cancel) -> Answer<Listing> {
        let (provider, folder) = (self.provider.clone(), parent.clone());
        let is_cancelled = || cancel.is_cancelled();
        let ask = Ask {
            key: parent,
            since,
            cancel: &is_cancelled,
        };
        self.flights.get(&ask, move || {
            list_folders(provider.as_ref(), &folder, &CancelToken::new())
        })
    }
}

pub struct PathsSource {
    listings: PathListings,
    env: Env,
    opened: Instant,
    offline: Mutex<Option<(u64, SharedString)>>,
}

impl PathsSource {
    #[must_use]
    pub fn new(listings: PathListings, env: Env) -> Self {
        Self {
            opened: listings.flights.now(),
            listings,
            env,
            offline: Mutex::new(None),
        }
    }

    #[must_use]
    pub fn with_wait(provider: Arc<dyn Provider>, env: Env, wait: Duration) -> Self {
        Self::new(
            PathListings::with_clock(provider, wait, Arc::new(Instant::now)),
            env,
        )
    }

    fn offline(&self) -> MutexGuard<'_, Option<(u64, SharedString)>> {
        self.offline.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn note(&self, generation: u64, location: &Location) {
        let path = SharedString::from(location.as_str().to_owned());
        *self.offline() = Some((generation, path));
    }
}

fn list_folders(provider: &dyn Provider, parent: &Location, token: &CancelToken) -> Listing {
    let mut found = Vec::new();
    let mut sink = |batch: rusty_fs::EntryBatch| {
        for entry in batch.entries {
            let folder = entry.kind == EntryKind::Folder
                || (entry.kind == EntryKind::Link && entry.attributes.link_to_folder);
            if let (true, Ok(location)) = (folder, parent.join(&entry.name)) {
                found.push(location);
            }
        }
        if token.is_cancelled() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    match provider.list(parent, &mut sink, token) {
        Ok(()) => Listing::Folders(found),
        Err(error) if matches!(error.kind, FsErrorKind::NetworkLost | FsErrorKind::Timeout) => {
            Listing::Unreachable
        }
        Err(_) => Listing::Failed,
    }
}

pub(crate) fn split(input: &str, here: &Location, env: &Env) -> Option<Split> {
    let cut = input.rfind(['\\', '/']).map_or(input.len(), |at| at + 1);
    let (head, tail) = input.split_at(cut);
    let (head, tail) = if matches!(tail, "." | "..") {
        (input, "")
    } else {
        (head, tail)
    };
    let lookup = |name: &str| env(name);
    let parent = typed_location(head, here, &lookup).ok()?;
    let typed = if tail.is_empty() {
        parent.clone()
    } else {
        typed_location(input, here, &lookup).ok()?
    };
    Some(Split {
        parent,
        typed,
        filter: tail.to_owned(),
    })
}

fn children(split: &Split, folders: Vec<Location>) -> Vec<Hit> {
    let mut places: Vec<Place> = folders
        .into_iter()
        .map(|location| Place::new(location, true, 0.0))
        .collect();
    places.sort_by_cached_key(|place| place.name.to_lowercase());
    let mut fuzzy = Fuzzy::new(&split.filter, false);
    places
        .iter()
        .filter_map(|place| {
            let scored = if split.filter.is_empty() {
                (0, Vec::new())
            } else {
                let found = fuzzy.score(&place.name)?;
                let bonus = rank::name_bonus(&place.name, &split.filter);
                (
                    rank::score(i64::from(found.score), 0.0, bonus),
                    found.indices,
                )
            };
            Some(place.hit("paths", scored, place.open_target()))
        })
        .collect()
}

fn go_to(typed: &Location) -> Hit {
    Hit {
        source: "paths",
        group: Group::Folders,
        title: SharedString::from(t!("command-center-go-to", path = typed.as_str().to_owned())),
        detail: SharedString::default(),
        score: GO_TO_SCORE,
        indices: Vec::new(),
        target: Target::GoTo(typed.clone()),
        shortcut: None,
    }
}

#[must_use]
pub fn completion(input: &str, hit: &Hit) -> Option<String> {
    let (
        true,
        Target::Open {
            location,
            folder: true,
        },
    ) = (hit.source == "paths", &hit.target)
    else {
        return None;
    };
    let name = location.name()?.display().into_owned();
    let windows = input.contains('\\') || input.as_bytes().get(1) == Some(&b':');
    let separator = if windows { '\\' } else { '/' };
    let head = input.rfind(['\\', '/']).map_or("", |at| &input[..=at]);
    let head = if head.is_empty() {
        format!("{input}{separator}")
    } else {
        head.to_owned()
    };
    Some(format!("{head}{name}{separator}"))
}

impl ResultSource for PathsSource {
    fn id(&self) -> &'static str {
        "paths"
    }

    fn group(&self) -> Group {
        Group::Folders
    }

    fn scopes(&self) -> Scopes {
        Scopes::FILES
    }

    fn search(&self, request: &Request, sink: &HitSink, cancel: &Cancel) {
        *self.offline() = None;
        let (Some(input), Some(here)) = (&request.query.path, &request.here) else {
            return;
        };
        let Some(split) = split(input, here, &self.env) else {
            return;
        };
        let generation = request.generation;
        match self.listings.list(&split.parent, self.opened, cancel) {
            Answer::Ready(Listing::Folders(folders)) => {
                let exists = split.filter.is_empty()
                    || folders.iter().any(|folder| folder.same_place(&split.typed));
                if exists {
                    sink.push(generation, go_to(&split.typed));
                }
                for hit in children(&split, folders) {
                    sink.push(generation, hit);
                }
            }
            Answer::Ready(Listing::Unreachable) | Answer::Unreachable => {
                self.note(generation, &split.parent);
            }
            Answer::Busy(other) => self.note(generation, &other),
            Answer::Ready(Listing::Failed) | Answer::Full | Answer::Cancelled => {}
        }
    }

    fn offline(&self, request: &Request) -> Option<Offline> {
        let offline = self.offline();
        let (generation, path) = offline.as_ref()?;
        (*generation == request.generation).then(|| Offline { path: path.clone() })
    }
}
