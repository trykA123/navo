use super::body::{BodyKeys, Key};
use super::{Slot, Viewer};
use crate::commands::active;
use crate::prelude::*;
use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

/// The hint for `command` in `context` as the keymap shows it, or `None` when it has no key.
pub(super) fn hint(cx: &App, command: &str, context: &str, label: &'static str) -> Option<Key> {
    shown(cx, command, context).map(|key| (interned(key), label))
}

/// The hints of `(command, context, label)` rows that have a key.
pub(super) fn hints(cx: &App, rows: &[(&str, &str, &'static str)]) -> Vec<Key> {
    rows.iter()
        .filter_map(|(command, context, label)| hint(cx, command, context, label))
        .collect()
}

/// Two commands of one context shown as `first/second` under one label.
pub(super) fn pair(
    cx: &App,
    (first, second): (&str, &str),
    context: &str,
    label: &'static str,
) -> Option<Key> {
    let key = match (shown(cx, first, context), shown(cx, second, context)) {
        (Some(first), Some(second)) => format!("{first}/{second}"),
        (one, other) => one.or(other)?,
    };
    Some((interned(key), label))
}

/// The bar is narrow, so long key names take their short form.
fn shown(cx: &App, command: &str, context: &str) -> Option<String> {
    let key = active(cx).shown_in(command, context)?;
    Some(
        key.split('+')
            .map(|part| match part {
                "Escape" => "Esc",
                "Backspace" => "Bksp",
                "-" => "−",
                other => other,
            })
            .collect::<Vec<_>>()
            .join("+"),
    )
}

/// `Key` needs `&'static str`, so distinct texts are leaked once, up to `MAX_INTERNED`.
/// Past the cap the key is empty and the bar shows the label alone.
fn interned(text: String) -> &'static str {
    static SEEN: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
    let mut guard = SEEN.lock().unwrap_or_else(PoisonError::into_inner);
    let seen = guard.get_or_insert_with(HashSet::new);
    if let Some(found) = seen.get(text.as_str()) {
        return found;
    }
    if seen.len() >= MAX_INTERNED {
        return "";
    }
    let leaked: &'static str = Box::leak(text.into_boxed_str());
    seen.insert(leaked);
    leaked
}

/// The next/previous file and page keys joined, e.g. `N/P Space/Bksp`.
pub(super) fn next_previous(cx: &App) -> Option<Key> {
    let files = pair(cx, ("viewer.next_file", "viewer.previous_file"), VIEWER, "");
    let pages = pair(
        cx,
        ("lister.page_or_next_image", "lister.previous_image"),
        VIEWER,
        "",
    );
    let joined = [files, pages]
        .into_iter()
        .flatten()
        .map(|(key, _)| key)
        .collect::<Vec<_>>();
    (!joined.is_empty()).then(|| (interned(joined.join(" ")), NEXT_PREVIOUS_LABEL))
}

const MAX_INTERNED: usize = 512;
const VIEWER: &str = "Viewer";
const NEXT_PREVIOUS_LABEL: &str = "Next/Prev";
const SLOTS: [Slot; 3] = [Slot::Text, Slot::Hex, Slot::Rich];

impl Viewer {
    fn slot_keys(&self, cx: &App, alt: bool, taken: &[Key]) -> Vec<Key> {
        let current = self.kind.as_ref().map(|kind| kind.id());
        let is_taken = |key: &str| taken.iter().any(|(used, _)| *used == key);
        SLOTS
            .iter()
            .filter_map(|slot| self.next_in(*slot))
            .filter(|kind| Some(kind.id()) != current)
            .filter_map(|kind| {
                let (plain, with_alt) = kind.slot().commands();
                let plain = (!alt).then(|| shown(cx, plain, VIEWER)).flatten();
                let key = match plain {
                    Some(key) if !is_taken(&key) => key,
                    _ => shown(cx, with_alt, VIEWER)?,
                };
                Some((interned(key), kind.name()))
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
            keys.extend(hint(cx, "viewer.edit", VIEWER, "Edit"));
        }
        if self
            .body
            .as_ref()
            .is_some_and(|body| body.parts(cx).is_some())
        {
            keys.extend(hint(cx, "viewer.view_menu", VIEWER, "View"));
        }
        let slots = self.slot_keys(cx, editing || takes_letters, &keys);
        keys.extend(slots);
        let has_files = keys.iter().any(|(_, label)| *label == NEXT_PREVIOUS_LABEL);
        if !editing && !takes_letters && !has_files {
            keys.extend(hints(
                cx,
                &[
                    ("viewer.next_file", VIEWER, "Next file"),
                    ("viewer.previous_file", VIEWER, "Previous file"),
                ],
            ));
        }
        keys.extend(hint(cx, "viewer.back", VIEWER, "Close"));
        keys
    }
}

#[cfg(test)]
pub(super) fn rebind(cx: &mut gpui_kit::TestAppContext, command: &str, keys: &str) {
    rebind_all(cx, &[(command, keys)]);
}

/// Each call starts again from the built-in keymap, so several binds go in one overlay.
#[cfg(test)]
pub(super) fn rebind_all(cx: &mut gpui_kit::TestAppContext, binds: &[(&str, &str)]) {
    let text: String = binds
        .iter()
        .map(|(command, keys)| format!("[[bind]]\ncommand = \"{command}\"\nkeys = [{keys}]\n"))
        .collect();
    let keymap = crate::commands::Keymap::builtin()
        .overlay(&text)
        .expect("keymap")
        .keymap;
    cx.update(|cx| crate::commands::set_active(cx, keymap));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn the_close_hint_shows_the_user_key(cx: &mut TestAppContext) {
        rebind(cx, "viewer.back", "\"f8\"");
        let key = cx.update(|cx| hint(cx, "viewer.back", VIEWER, "Close"));
        assert_eq!(key, Some(("F8", "Close")));
    }

    #[gpui_kit::test]
    fn a_command_with_no_key_hides_its_hint(cx: &mut TestAppContext) {
        rebind(cx, "viewer.view_menu", "");
        let key = cx.update(|cx| hint(cx, "viewer.view_menu", VIEWER, "View"));
        assert_eq!(key, None);
    }

    #[gpui_kit::test]
    fn next_previous_joins_the_user_keys(cx: &mut TestAppContext) {
        rebind(cx, "viewer.next_file", "\"f7\"");
        let key = cx.update(|cx| next_previous(cx));
        assert_eq!(key, Some(("F7/P Space/Bksp", NEXT_PREVIOUS_LABEL)));
    }
}
