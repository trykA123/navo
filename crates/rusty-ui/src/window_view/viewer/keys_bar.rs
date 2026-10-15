use super::body::{BodyKeys, Key};
use super::{Slot, Viewer};
use crate::prelude::*;

const CLOSE: Key = ("Esc", "Close");
const FILES: [Key; 2] = [("N", "Next file"), ("P", "Previous file")];
const SLOTS: [Slot; 3] = [Slot::Text, Slot::Hex, Slot::Rich];

impl Viewer {
    fn slot_keys(&self, alt: bool) -> Vec<Key> {
        let shown = self.kind.as_ref().map(|kind| kind.id());
        SLOTS
            .iter()
            .filter_map(|slot| self.next_in(*slot))
            .filter(|kind| Some(kind.id()) != shown)
            .map(|kind| {
                let slot = kind.slot();
                let key = if alt { slot.alt_key() } else { slot.key() };
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
        if self.can_start_editing(cx) {
            keys.push(("Ctrl+E", "Edit"));
        }
        keys.extend(self.slot_keys(editing));
        if !editing {
            keys.extend(FILES);
        }
        keys.push(CLOSE);
        keys
    }
}
