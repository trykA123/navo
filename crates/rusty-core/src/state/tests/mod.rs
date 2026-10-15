use super::{Entry, Listing, Tab};
use crate::{Location, TabId};

mod app;
mod history;
mod listing;
mod pane;
mod props;
mod tab;
mod windows;

fn loc(text: &str) -> Location {
    Location::new(text).unwrap()
}

fn entry(name: &str, is_folder: bool) -> Entry {
    Entry {
        name: name.to_owned(),
        is_folder,
    }
}

fn listing(len: usize) -> Listing {
    Listing::new((0..len).map(|i| entry(&format!("f{i}"), false)).collect())
}

fn tab(raw: u64, location: &str) -> Tab {
    Tab::new(TabId::new(raw), loc(location))
}
