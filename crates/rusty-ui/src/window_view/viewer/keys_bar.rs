use super::body::{BodyKeys, Key};
use super::{Slot, Viewer};
use crate::prelude::*;

const CLOSE: Key = ("Esc", "Close");
const FILES: [Key; 2] = [("N", "Next file"), ("P", "Previous file")];
pub(super) const NEXT_PREVIOUS: Key = ("N/P Space/Bksp", "Next/Prev");
const SLOTS: [Slot; 3] = [Slot::Text, Slot::Hex, Slot::Rich];

impl Viewer {
    fn slot_keys(&self, alt: bool, taken: &[Key]) -> Vec<Key> {
        let shown = self.kind.as_ref().map(|kind| kind.id());
        let is_taken = |key: &str| taken.iter().any(|(used, _)| *used == key);
        SLOTS
            .iter()
            .filter_map(|slot| self.next_in(*slot))
            .filter(|kind| Some(kind.id()) != shown)
            .map(|kind| {
                let slot = kind.slot();
                let key = if alt || is_taken(slot.key()) {
                    slot.alt_key()
                } else {
                    slot.key()
                };
                (key, kind.name())
            })
            .collect()
    }

    pub(in crate::window_view) fn keys(&self, cx: &App) -> Vec<Key> {
        let mut keys = match self.body.as_ref().map(|body| body.keys(cx)) {
            Some(BodyKeys::Modal(keys)) => return keys,
            Some(BodyKeys::Normal(keys)) => keys,
            None => Vec::new(),
        };
        let editing = self.is_editing(cx);
        let takes_letters = self
            .body
            .as_ref()
            .is_some_and(|body| body.takes_letters(cx));
        if self.can_start_editing(cx) {
            keys.push(("Ctrl+E", "Edit"));
        }
        let slots = self.slot_keys(editing || takes_letters, &keys);
        keys.extend(slots);
        if !editing && !takes_letters && !keys.contains(&NEXT_PREVIOUS) {
            keys.extend(FILES);
        }
        keys.push(CLOSE);
        keys
    }
}
