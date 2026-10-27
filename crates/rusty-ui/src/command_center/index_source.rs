mod metadata;
mod options;
mod pipe;

use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use gpui_kit::SharedString;
use rusty_core::{FsError, FsErrorKind, Location};
use rusty_fs::FsEntry;
use rusty_index::pipe::{self as protocol, Reply};
use rusty_index::table::Flags;

use super::file_sources::Frecents;
use super::rank;
use super::{Cancel, Group, Hit, HitSink, Request, ResultSource, Scopes, Target, Term};
pub use metadata::IndexMetadata;
use metadata::Shown;
pub use options::{IndexMode, IndexOptions, drive_root};
pub use pipe::{PipeAnswer, PipeQueries};

pub type HostQuery = Arc<
    dyn Fn(&IndexOptions, &protocol::Query, &Cancel, Option<&protocol::Status>) -> Reply
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct IndexHooks {
    pub start: Arc<dyn Fn(&IndexOptions) + Send + Sync>,
    pub status: Arc<dyn Fn() -> Option<Indexing> + Send + Sync>,
    pub pipe: PipeQueries,
    pub host: HostQuery,
    pub metadata: IndexMetadata,
}

impl IndexHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            start: Arc::new(|_| {}),
            status: Arc::new(|| None),
            pipe: PipeQueries::new(Arc::new(|_, _, _| Err(protocol::PipeError::NotAvailable))),
            host: Arc::new(|_, _, _, _| Reply::default()),
            metadata: IndexMetadata::new(Arc::new(|_| {
                Err(FsError {
                    kind: FsErrorKind::NotFound,
                    location: None,
                    os_code: None,
                    holders: Vec::new(),
                })
            })),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Indexing {
    pub root: SharedString,
    pub count: u64,
    pub busy: bool,
}

impl Indexing {
    fn busy() -> Self {
        Self {
            root: SharedString::default(),
            count: 0,
            busy: true,
        }
    }

    #[must_use]
    pub fn text(&self) -> SharedString {
        if self.busy {
            return rusty_l10n::t!("command-center-index-busy").into();
        }
        rusty_l10n::t!(
            "command-center-indexing",
            root = self.root.to_string(),
            count = crate::format::bytes(self.count)
        )
        .into()
    }
}

pub struct IndexSource {
    options: IndexOptions,
    hooks: IndexHooks,
    load: Frecents,
    frecency: OnceLock<Vec<(Location, f64)>>,
    note: Mutex<Option<(u64, Option<Indexing>)>>,
    shown: Shown,
}

impl IndexSource {
    #[must_use]
    pub fn new(options: IndexOptions, hooks: IndexHooks, load: Frecents) -> Self {
        Self {
            options,
            hooks,
            load,
            frecency: OnceLock::new(),
            note: Mutex::new(None),
            shown: Shown::default(),
        }
    }

    fn weight(&self, location: &Location) -> f64 {
        self.frecency
            .get_or_init(|| {
                (self.load)(2_000)
                    .into_iter()
                    .filter_map(|row| Some((Location::new(row.location).ok()?, row.weight)))
                    .collect()
            })
            .iter()
            .find(|(path, _)| path.same_place(location))
            .map_or(0.0, |(_, weight)| *weight)
    }

    fn query(&self, request: &Request, cancel: &Cancel) -> Option<Reply> {
        let query = protocol::Query {
            generation: request.generation,
            request: protocol::Request {
                terms: request
                    .query
                    .terms
                    .iter()
                    .map(|term| match term {
                        Term::Name(text) => protocol::QueryTerm::Name(text.clone()),
                        Term::Path(text) => protocol::QueryTerm::Path(text.clone()),
                        Term::Ext(list) => protocol::QueryTerm::Ext(list.clone()),
                    })
                    .collect(),
            },
        };
        let answer = match self.options.mode {
            IndexMode::Auto | IndexMode::Service => {
                self.hooks
                    .pipe
                    .query(self.options.clone(), query.clone(), cancel)
            }
            IndexMode::Walk | IndexMode::Off => PipeAnswer::Absent,
        };
        let reply = match answer {
            PipeAnswer::Replied(reply) => Some(reply),
            PipeAnswer::Silent if self.options.mode == IndexMode::Auto => return None,
            PipeAnswer::Silent | PipeAnswer::Absent => None,
        };
        if cancel.is_cancelled() || !matches!(self.options.mode, IndexMode::Auto | IndexMode::Walk)
        {
            return Some(reply.unwrap_or_default());
        }
        let walked = (self.hooks.host)(
            &self.options,
            &query,
            cancel,
            reply.as_ref().map(|reply| &reply.status),
        );
        let mut reply = reply.unwrap_or_default();
        reply.hits.extend(walked.hits);
        reply.status.volumes.extend(walked.status.volumes);
        Some(reply)
    }

    fn hit(&self, found: protocol::Hit, words: &str) -> Option<Hit> {
        let location = Location::new(&found.path).ok()?;
        if !self.options.admits(&location, found.flags) {
            return None;
        }
        let title = location.name()?.display().into_owned();
        let folder = found.flags & Flags::FOLDER.bits() != 0;
        Some(Hit {
            source: "index",
            group: if folder { Group::Folders } else { Group::Files },
            detail: location
                .parent()
                .map(|parent| parent.as_str().to_owned())
                .unwrap_or_default()
                .into(),
            score: rank::score(
                i64::from(found.score),
                self.weight(&location),
                rank::name_bonus(&title, words),
            ),
            title: title.into(),
            indices: found.indices,
            target: Target::Open { location, folder },
            shortcut: None,
        })
    }

    fn remember_status(&self, generation: u64, status: protocol::Status) {
        let note = status
            .volumes
            .into_iter()
            .find(|volume| volume.progress.is_some())
            .map(|volume| {
                let root = volume.root.trim_end_matches(['\\', '/']);
                Indexing {
                    root: if root.is_empty() {
                        volume.root.clone()
                    } else {
                        root.to_owned()
                    }
                    .into(),
                    count: volume.entries,
                    busy: false,
                }
            });
        self.remember_note(generation, note);
    }

    fn remember_note(&self, generation: u64, note: Option<Indexing>) {
        let mut seen = self.note.lock().unwrap_or_else(PoisonError::into_inner);
        if seen.as_ref().is_none_or(|(old, _)| *old <= generation) {
            *seen = Some((generation, note));
        }
    }

    #[must_use]
    pub fn metadata(&self, location: &Location) -> Option<FsEntry> {
        self.shown.get(location)
    }
}

impl ResultSource for IndexSource {
    fn id(&self) -> &'static str {
        "index"
    }
    fn group(&self) -> Group {
        Group::Files
    }
    fn scopes(&self) -> Scopes {
        Scopes::FILES
    }

    fn search(&self, request: &Request, sink: &HitSink, cancel: &Cancel) {
        if self.options.mode == IndexMode::Off
            || cancel.is_cancelled()
            || request.query.path.is_some()
            || request.query.terms.is_empty()
            || !self.scopes().contains(request.query.scope)
        {
            return;
        }
        let reply = self.query(request, cancel);
        if cancel.is_cancelled() {
            return;
        }
        let Some(reply) = reply else {
            self.remember_note(request.generation, Some(Indexing::busy()));
            return;
        };
        self.remember_status(request.generation, reply.status);
        let words = request
            .query
            .terms
            .iter()
            .filter_map(|term| match term {
                Term::Name(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        let mut hits: Vec<_> = deduplicate(reply.hits)
            .into_iter()
            .filter_map(|hit| self.hit(hit, &words))
            .collect();
        hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
        for hit in hits.into_iter().take(request.limit) {
            if cancel.is_cancelled() {
                return;
            }
            sink.push(request.generation, hit);
        }
    }

    fn indexing(&self, request: &Request) -> Option<Indexing> {
        self.note
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|(generation, _)| *generation == request.generation)
            .and_then(|(_, note)| note.clone())
    }

    fn wait_for_more(&self, request: &Request, cancel: &Cancel) -> bool {
        if self.indexing(request).is_none_or(|note| note.busy) {
            return false;
        }
        for _ in 0..10 {
            if cancel.is_cancelled() {
                return false;
            }
            std::thread::park_timeout(Duration::from_millis(10));
        }
        !cancel.is_cancelled()
    }

    fn shown(&self, hits: &[Hit], cancel: &Cancel) {
        self.shown.load(&self.hooks.metadata, hits, cancel);
    }
}

fn deduplicate(hits: Vec<protocol::Hit>) -> Vec<protocol::Hit> {
    let mut places: Vec<(Location, protocol::Hit)> = Vec::new();
    for hit in hits {
        let Ok(location) = Location::new(&hit.path) else {
            continue;
        };
        if let Some((_, kept)) = places
            .iter_mut()
            .find(|(place, _)| place.same_place(&location))
        {
            if hit.score > kept.score {
                *kept = hit;
            }
            continue;
        }
        places.push((location, hit));
    }
    places.into_iter().map(|(_, hit)| hit).collect()
}
