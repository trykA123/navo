use rusty_core::{AppState, Location};

use super::{Matcher, Place};
use crate::command_center::source::{
    Cancel, Group, HitSink, Request, ResultSource, Scopes, Target,
};

#[derive(Clone, Debug, Default)]
pub struct TabsSnapshot {
    places: Vec<Place>,
}

impl TabsSnapshot {
    #[must_use]
    pub fn of(state: &AppState) -> Self {
        let locations = state
            .window_ids()
            .into_iter()
            .filter_map(|id| state.window(id))
            .flat_map(|window| window.panes())
            .flat_map(|pane| pane.tabs())
            .map(|tab| tab.location().clone());
        Self::from_locations(locations)
    }

    #[must_use]
    pub fn from_locations(locations: impl IntoIterator<Item = Location>) -> Self {
        Self {
            places: locations
                .into_iter()
                .map(|location| Place::new(location, true, 0.0))
                .collect(),
        }
    }
}

pub struct TabsSource {
    snapshot: TabsSnapshot,
}

impl TabsSource {
    #[must_use]
    pub fn new(snapshot: TabsSnapshot) -> Self {
        Self { snapshot }
    }
}

impl ResultSource for TabsSource {
    fn id(&self) -> &'static str {
        "tabs"
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
        for place in &self.snapshot.places {
            if cancel.is_cancelled() {
                return;
            }
            if let Some((score, indices)) = matcher.score(place) {
                let target = Target::GoTo(place.location.clone());
                sink.push(
                    request.generation,
                    place.hit("tabs", (score, indices), target),
                );
            }
        }
    }
}
