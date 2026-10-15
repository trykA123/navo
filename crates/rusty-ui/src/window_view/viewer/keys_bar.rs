use super::{Slot, Viewer};
use crate::prelude::*;

type Key = (&'static str, &'static str);

const CLOSE: Key = ("Esc", "Close");
const FIND: [Key; 2] = [("Ctrl+F", "Find"), ("F3", "Next")];
const FILES: [Key; 2] = [("N", "Next file"), ("P", "Previous file")];
const SLOTS: [Slot; 3] = [Slot::Text, Slot::Hex, Slot::Rich];

impl Viewer {
    fn slot_keys(&self, alt: bool) -> Vec<Key> {
        let shown = self.kind.as_ref().map(|kind| kind.id());
        SLOTS
            .iter()
            .filter_map(|slot| self.kind_in(*slot))
            .filter(|kind| Some(kind.id()) != shown)
            .map(|kind| {
                let slot = kind.slot();
                let key = if alt { slot.alt_key() } else { slot.key() };
                (key, kind.name())
            })
            .collect()
    }

    pub(in crate::window_view) fn keys(&self, cx: &App) -> Vec<Key> {
        let paged = self.paged.as_ref().map(|paged| paged.read(cx));
        if let Some(keys) = paged.and_then(|paged| paged.bar_keys()) {
            return keys.to_vec();
        }
        let mut keys = Vec::new();
        if self.editor.is_some() || paged.is_some() {
            keys.extend(FIND);
        }
        if self.editing {
            keys.push(("Ctrl+S", "Save"));
            keys.extend(self.slot_keys(true));
            keys.push(CLOSE);
            return keys;
        }
        keys.extend(paged.map(|paged| paged.go_to_key()));
        if self.can_start_editing() {
            keys.push(("Ctrl+E", "Edit"));
        }
        keys.extend(self.slot_keys(false));
        keys.extend(FILES);
        keys.push(CLOSE);
        keys
    }
}
