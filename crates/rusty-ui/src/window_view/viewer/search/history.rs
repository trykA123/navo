use rusty_core::marks::SearchHistory;
use rusty_core::paging::Needle;

use crate::prelude::*;
use crate::primitives::TextFieldState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view::viewer) enum NeedleKind {
    Hex,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct Recalled {
    pub text: String,
    pub match_case: Option<bool>,
}

pub(in crate::window_view::viewer) struct Recall<'a> {
    pub older: bool,
    pub kind: NeedleKind,
    pub input: &'a Entity<TextFieldState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Walk {
    at: usize,
    draft: String,
}

#[derive(Debug, Default)]
pub(in crate::window_view::viewer) struct History {
    entries: SearchHistory,
    walk: Option<Walk>,
}

pub(in crate::window_view::viewer) fn needle_text(needle: &Needle) -> String {
    match needle {
        Needle::Text { text, .. } => text.clone(),
        Needle::Bytes(bytes) => bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn recalled(needle: &Needle) -> Recalled {
    let match_case = match needle {
        Needle::Text { match_case, .. } => Some(*match_case),
        Needle::Bytes(_) => None,
    };
    Recalled {
        text: needle_text(needle),
        match_case,
    }
}

fn is_kind(needle: &Needle, kind: NeedleKind) -> bool {
    matches!(
        (needle, kind),
        (Needle::Text { .. }, NeedleKind::Text) | (Needle::Bytes(_), NeedleKind::Hex)
    )
}

impl History {
    pub(in crate::window_view::viewer) fn push(&mut self, needle: Needle) -> bool {
        self.walk = None;
        if self.entries.iter().next() == Some(&needle) {
            return false;
        }
        self.entries.push(needle);
        true
    }

    pub(in crate::window_view::viewer) fn merge_older(&mut self, older: SearchHistory) {
        let newer: Vec<Needle> = self.entries.iter().cloned().collect();
        self.entries = older;
        for needle in newer.into_iter().rev() {
            self.entries.push(needle);
        }
    }

    pub(in crate::window_view::viewer) fn recall_into(
        &mut self,
        recall: Recall<'_>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Option<bool>> {
        let typed = recall.input.read(cx).value().to_string();
        let recalled = self.step(recall.older, recall.kind, &typed)?;
        recall
            .input
            .update(cx, |state, cx| state.set_value(recalled.text, window, cx));
        Some(recalled.match_case)
    }

    fn of_kind(&self, kind: NeedleKind) -> Vec<&Needle> {
        self.entries
            .iter()
            .filter(|needle| is_kind(needle, kind))
            .collect()
    }

    pub(in crate::window_view::viewer) fn step(
        &mut self,
        older: bool,
        kind: NeedleKind,
        typed: &str,
    ) -> Option<Recalled> {
        let entries = self.of_kind(kind);
        let next = match (&self.walk, older) {
            (None, true) => Some(0),
            (None, false) => None,
            (Some(walk), true) => Some(walk.at + 1),
            (Some(walk), false) => walk.at.checked_sub(1),
        };
        let Some(at) = next else {
            let draft = self.walk.take()?.draft;
            return Some(Recalled {
                text: draft,
                match_case: None,
            });
        };
        let found = recalled(entries.get(at)?);
        let draft = self
            .walk
            .take()
            .map_or_else(|| typed.to_owned(), |walk| walk.draft);
        self.walk = Some(Walk { at, draft });
        Some(found)
    }
}

#[cfg(test)]
mod tests {
    use rusty_core::marks::SearchHistory;
    use rusty_core::paging::Needle;

    use super::{History, NeedleKind, needle_text};

    fn text(value: &str) -> Needle {
        Needle::Text {
            text: value.to_owned(),
            match_case: false,
        }
    }

    fn walked(history: &mut History, older: bool) -> Option<String> {
        history
            .step(older, NeedleKind::Text, "draft")
            .map(|recalled| recalled.text)
    }

    #[test]
    fn up_walks_older_and_down_comes_back_to_the_draft() {
        let mut history = History::default();
        for word in ["first", "second", "third"] {
            assert!(history.push(text(word)));
        }
        assert_eq!(walked(&mut history, true).as_deref(), Some("third"));
        assert_eq!(walked(&mut history, true).as_deref(), Some("second"));
        assert_eq!(walked(&mut history, false).as_deref(), Some("third"));
        assert_eq!(walked(&mut history, false).as_deref(), Some("draft"));
        assert_eq!(walked(&mut history, false), None);
    }

    #[test]
    fn the_walk_stops_at_the_oldest_entry() {
        let mut history = History::default();
        history.push(text("only"));
        assert_eq!(walked(&mut history, true).as_deref(), Some("only"));
        assert_eq!(walked(&mut history, true), None);
        assert_eq!(walked(&mut history, false).as_deref(), Some("draft"));
    }

    #[test]
    fn hex_mode_offers_only_hex_entries() {
        let mut history = History::default();
        history.push(Needle::Bytes(vec![0x4D, 0x5A]));
        history.push(text("words"));
        let recalled = history.step(true, NeedleKind::Hex, "");
        assert_eq!(
            recalled.map(|recalled| recalled.text).as_deref(),
            Some("4D 5A")
        );
    }

    #[test]
    fn local_searches_stay_newer_than_the_loaded_history() {
        let mut history = History::default();
        history.push(text("local"));
        let mut loaded = SearchHistory::default();
        loaded.push(text("saved"));
        history.merge_older(loaded);
        assert_eq!(walked(&mut history, true).as_deref(), Some("local"));
        assert_eq!(walked(&mut history, true).as_deref(), Some("saved"));
    }

    #[test]
    fn bytes_read_as_spaced_hex() {
        assert_eq!(needle_text(&Needle::Bytes(vec![0x0A, 0xFF])), "0A FF");
    }
}
