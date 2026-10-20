mod recent;
mod tabs;

use gpui_kit::SharedString;
use rusty_core::Location;
use rusty_index::fuzzy::Fuzzy;

use super::query::{Query, Term};
use super::rank;
use super::source::{Group, Hit, Target};

pub use recent::{Frecents, RecentSource};
pub use tabs::{TabsSnapshot, TabsSource};

#[derive(Clone, Debug)]
struct Place {
    location: Location,
    name: String,
    parent: String,
    folder: bool,
    weight: f64,
}

impl Place {
    fn new(location: Location, folder: bool, weight: f64) -> Self {
        let name = location.name().map_or_else(
            || location.as_str().to_owned(),
            |name| name.display().into_owned(),
        );
        let parent = location
            .parent()
            .map(|parent| parent.as_str().to_owned())
            .unwrap_or_default();
        Self {
            location,
            name,
            parent,
            folder,
            weight,
        }
    }

    fn extension(&self) -> Option<String> {
        if self.folder {
            return None;
        }
        let (_, ext) = self.name.rsplit_once('.')?;
        Some(ext.to_lowercase())
    }

    fn hit(&self, source: &'static str, (score, indices): (i64, Vec<u32>), target: Target) -> Hit {
        Hit {
            source,
            group: if self.folder {
                Group::Folders
            } else {
                Group::Files
            },
            title: SharedString::from(self.name.clone()),
            detail: SharedString::from(self.parent.clone()),
            score,
            indices,
            target,
            shortcut: None,
        }
    }
}

enum Piece {
    Name(Fuzzy),
    Path(Fuzzy),
    Ext(Vec<String>),
}

impl Piece {
    fn score(&mut self, place: &Place) -> Option<(i64, Vec<u32>)> {
        match self {
            Self::Name(fuzzy) => {
                let found = fuzzy.score(&place.name)?;
                Some((i64::from(found.score), found.indices))
            }
            Self::Path(fuzzy) => {
                let found = fuzzy.score(place.location.as_str())?;
                Some((i64::from(found.score), Vec::new()))
            }
            Self::Ext(list) => {
                let ext = place.extension()?;
                list.contains(&ext).then(|| (0, Vec::new()))
            }
        }
    }
}

struct Matcher {
    pieces: Vec<Piece>,
    words: String,
}

impl Matcher {
    fn of(query: &Query) -> Option<Self> {
        if query.path.is_some() || query.terms.is_empty() {
            return None;
        }
        let mut words = Vec::new();
        let pieces = query
            .terms
            .iter()
            .map(|term| match term {
                Term::Name(text) => {
                    words.push(text.as_str());
                    Piece::Name(Fuzzy::new(text, false))
                }
                Term::Path(text) => Piece::Path(Fuzzy::new(text, true)),
                Term::Ext(list) => Piece::Ext(list.clone()),
            })
            .collect();
        Some(Self {
            pieces,
            words: words.join(" "),
        })
    }

    fn score(&mut self, place: &Place) -> Option<(i64, Vec<u32>)> {
        let mut total = 0_i64;
        let mut indices = Vec::new();
        for piece in &mut self.pieces {
            let (score, found) = piece.score(place)?;
            total += score;
            indices.extend(found);
        }
        indices.sort_unstable();
        indices.dedup();
        let bonus = rank::name_bonus(&place.name, &self.words);
        Some((rank::score(total, place.weight, bonus), indices))
    }
}
