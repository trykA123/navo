use std::sync::OnceLock;

use rusty_core::Location;

use super::{Matcher, Place};
use crate::command_center::query::Query;
use crate::command_center::source::{
    Cancel, Group, Hit, HitSink, Request, ResultSource, Scopes, Target,
};
use crate::window_view::Frecent;

pub const RECENT_ROWS: usize = 2_000;

pub type Frecents = std::sync::Arc<dyn Fn(usize) -> Vec<Frecent> + Send + Sync>;

pub struct RecentSource {
    load: Frecents,
    places: OnceLock<Vec<Place>>,
}

impl RecentSource {
    #[must_use]
    pub fn new(load: Frecents) -> Self {
        Self {
            load,
            places: OnceLock::new(),
        }
    }

    fn places(&self) -> &[Place] {
        self.places.get_or_init(|| {
            (self.load)(RECENT_ROWS)
                .into_iter()
                .filter_map(|row| {
                    let location = Location::new(row.location).ok()?;
                    Some(Place::new(location, row.is_folder, row.weight))
                })
                .collect()
        })
    }

    #[must_use]
    pub fn top_folders(&self, count: usize) -> Vec<Hit> {
        self.places()
            .iter()
            .filter(|place| place.folder)
            .take(count)
            .map(|place| place.hit("recent", (0, Vec::new()), target(place)))
            .collect()
    }

    #[must_use]
    pub fn matches(&self, query: &Query) -> Vec<Hit> {
        let Some(mut matcher) = Matcher::of(query) else {
            return Vec::new();
        };
        self.places()
            .iter()
            .filter_map(|place| {
                let (score, indices) = matcher.score(place)?;
                Some(place.hit("recent", (score, indices), target(place)))
            })
            .collect()
    }
}

fn target(place: &Place) -> Target {
    Target::Open {
        location: place.location.clone(),
        folder: place.folder,
    }
}

impl ResultSource for RecentSource {
    fn id(&self) -> &'static str {
        "recent"
    }

    fn group(&self) -> Group {
        Group::Folders
    }

    fn scopes(&self) -> Scopes {
        Scopes::FILES
    }

    fn search(&self, request: &Request, sink: &HitSink, cancel: &Cancel) {
        let Some(mut matcher) = Matcher::of(&request.query) else {
            return;
        };
        for place in self.places() {
            if cancel.is_cancelled() {
                return;
            }
            if let Some((score, indices)) = matcher.score(place) {
                sink.push(
                    request.generation,
                    place.hit("recent", (score, indices), target(place)),
                );
            }
        }
    }
}
