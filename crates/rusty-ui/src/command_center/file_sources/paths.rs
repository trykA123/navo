use std::ops::ControlFlow;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use gpui_kit::SharedString;
use rusty_core::{FsErrorKind, Location, typed_location};
use rusty_fs::{CancelToken, EntryKind, Provider};
use rusty_index::fuzzy::Fuzzy;
use rusty_l10n::t;

use super::Place;
use crate::command_center::rank;
use crate::command_center::source::{
    Cancel, Group, Hit, HitSink, Offline, Request, ResultSource, Scopes, Target,
};

pub const LIST_WAIT: Duration = Duration::from_secs(2);
const GO_TO_SCORE: i64 = 1_000_000;
const POLL: Duration = Duration::from_millis(20);

pub type Env = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub(crate) struct Split {
    pub(crate) parent: Location,
    pub(crate) typed: Location,
    pub(crate) filter: String,
}

enum Listing {
    Folders(Vec<Location>),
    Unreachable,
    Failed,
}

pub struct PathsSource {
    provider: Arc<dyn Provider>,
    env: Env,
    wait: Duration,
    offline: Mutex<Option<(u64, SharedString)>>,
}

impl PathsSource {
    #[must_use]
    pub fn new(provider: Arc<dyn Provider>, env: Env) -> Self {
        Self::with_wait(provider, env, LIST_WAIT)
    }

    #[must_use]
    pub fn with_wait(provider: Arc<dyn Provider>, env: Env, wait: Duration) -> Self {
        Self {
            provider,
            env,
            wait,
            offline: Mutex::new(None),
        }
    }

    fn offline(&self) -> MutexGuard<'_, Option<(u64, SharedString)>> {
        self.offline.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn list(&self, parent: &Location, cancel: &Cancel) -> Listing {
        let (sender, receiver) = mpsc::channel();
        let (provider, folder, token) = (self.provider.clone(), parent.clone(), CancelToken::new());
        let job = token.clone();
        let spawned = thread::Builder::new()
            .name("paths-list".into())
            .spawn(move || {
                let _ = sender.send(list_folders(provider.as_ref(), &folder, &job));
            });
        if spawned.is_err() {
            return Listing::Failed;
        }
        let deadline = Instant::now() + self.wait;
        loop {
            match receiver.recv_timeout(POLL) {
                Ok(listing) => return listing,
                Err(RecvTimeoutError::Disconnected) => return Listing::Failed,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if cancel.is_cancelled() {
                token.cancel();
                return Listing::Failed;
            }
            if Instant::now() >= deadline {
                token.cancel();
                return Listing::Unreachable;
            }
        }
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
        match self.list(&split.parent, cancel) {
            Listing::Folders(folders) => {
                let exists = split.filter.is_empty()
                    || folders.iter().any(|folder| folder.same_place(&split.typed));
                if exists {
                    sink.push(request.generation, go_to(&split.typed));
                }
                for hit in children(&split, folders) {
                    sink.push(request.generation, hit);
                }
            }
            Listing::Unreachable => {
                let path = SharedString::from(split.parent.as_str().to_owned());
                *self.offline() = Some((request.generation, path));
            }
            Listing::Failed => {}
        }
    }

    fn offline(&self, request: &Request) -> Option<Offline> {
        let offline = self.offline();
        let (generation, path) = offline.as_ref()?;
        (*generation == request.generation).then(|| Offline { path: path.clone() })
    }
}
