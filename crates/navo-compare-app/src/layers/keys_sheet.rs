//! The Keys sheet (`?` or F1): a modal list of every shortcut with a live filter.
//! The shell routes its actions through `wire`.

use gpui_kit::base::TestTextExt as _;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{FocusHandle, KeyBinding, ReadGlobal, Subscription};
use sidewise_kit::prelude::*;
use sidewise_kit::primitives::{Kbd, TextField, TextFieldState, text_field_state};
use sidewise_kit::shown::shown;

use super::keys_sheet_compare::compare_groups;
use crate::theme::SidewiseTheme;
use crate::theme_data::palettes::ModeTokens;

gpui_kit::actions!(layers, [ToggleKeys, CloseKeys]);

/// Key context of the open sheet; Escape is bound inside it.
const SHEET: &str = "KeysSheet";
/// One row of `GROUPS`: a label and its keys in gpui form.
type Row = (&'static str, &'static [&'static str]);

/// The keys that exist now, as `(group title, rows)`; a row is a label and its keys in gpui form.
const GROUPS: [(&str, &[Row]); 4] = [
    (
        "Window",
        &[
            ("New tab", &["ctrl-t"]),
            ("Close tab", &["ctrl-w"]),
            ("Next tab", &["ctrl-tab"]),
            ("Previous tab", &["ctrl-shift-tab"]),
            ("Show or hide the sidebar", &["ctrl-\\"]),
            ("Move between sidebar and tab", &["f6", "shift-f6"]),
            ("Show keyboard shortcuts", &["?", "f1"]),
        ],
    ),
    (
        "Compare tab",
        &[
            ("New tab", &["1"]),
            ("Folders", &["2"]),
            ("File", &["3"]),
            ("Stream", &["4"]),
            ("Large", &["5"]),
            ("Compare again", &["f5"]),
            ("Back to Folders", &["escape"]),
        ],
    ),
    (
        "Folders",
        &[
            ("Move the cursor", &["up", "down"]),
            ("Step out or in", &["left", "right"]),
            ("Open the row", &["enter"]),
        ],
    ),
    ("Large", &[("Show Folders", &["enter"])]),
];

/// One line of the sheet: a command's label and its keys in gpui form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyRow {
    pub label: SharedString,
    pub keys: Vec<String>,
}

/// A titled run of rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyGroup {
    pub title: SharedString,
    pub rows: Vec<KeyRow>,
}

/// Binds `?` and F1 outside text fields, and Escape inside the open sheet.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("?", ToggleKeys, Some("Shell && !Input")),
        KeyBinding::new("f1", ToggleKeys, Some("Shell")),
        KeyBinding::new("escape", CloseKeys, Some(SHEET)),
    ]);
}

/// Hooks the open and close actions onto `root`, so the shell opens the sheet.
pub fn wire(root: Div, keys: &Entity<KeysSheet>) -> Div {
    let sheet = keys.clone();
    root.on_action(move |_: &ToggleKeys, window, cx| {
        sheet.update(cx, |sheet, cx| sheet.toggle(window, cx));
    })
}

/// The static groups, then the compare groups read from `sidewise-ui` (NC114).
pub fn key_groups() -> Vec<KeyGroup> {
    let row = |(label, keys): &Row| KeyRow {
        label: (*label).to_owned().into(),
        keys: keys.iter().map(|key| (*key).to_owned()).collect(),
    };
    let fixed = GROUPS.iter().map(|(title, rows)| KeyGroup {
        title: (*title).into(),
        rows: rows.iter().map(row).collect(),
    });
    fixed.chain(compare_groups()).collect()
}

/// The rows whose label or shown keys contain `query` (case-insensitive); empty groups drop out.
pub fn filter_groups(groups: &[KeyGroup], query: &str) -> Vec<KeyGroup> {
    let query = query.trim().to_lowercase();
    let hit = |row: &KeyRow| {
        row.label.to_lowercase().contains(&query)
            || row
                .keys
                .iter()
                .any(|key| shown(key).is_some_and(|text| text.to_lowercase().contains(&query)))
    };
    groups
        .iter()
        .filter_map(|group| {
            let rows: Vec<KeyRow> = group.rows.iter().filter(|row| hit(row)).cloned().collect();
            (!rows.is_empty()).then_some(KeyGroup {
                title: group.title.clone(),
                rows,
            })
        })
        .collect()
}

/// The active mode's tokens; the drawer host reads them too.
pub(super) fn tokens_of(cx: &App) -> (SidewiseTheme, ModeTokens) {
    let side = *SidewiseTheme::global(cx);
    let tones = if side.mode.is_dark() {
        side.spec.dark
    } else {
        side.spec.light
    };
    (side, tones)
}

/// The Keys sheet: a centred modal over the scrim with a live shortcut filter.
pub struct KeysSheet {
    open: bool,
    groups: Vec<KeyGroup>,
    query: Entity<TextFieldState>,
    /// Where focus was before the sheet opened; Escape gives it back.
    restore: Option<FocusHandle>,
    _typing: Subscription,
}

impl KeysSheet {
    /// A closed sheet over `key_groups()`.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = text_field_state("Filter shortcuts", window, cx);
        let typing = cx.subscribe(&query, |_, _, _: &InputEvent, cx| cx.notify());
        Self {
            open: false,
            groups: key_groups(),
            query,
            restore: None,
            _typing: typing,
        }
    }

    /// Whether the sheet is showing.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens a closed sheet (remembering focus, focusing the filter) or closes an open one.
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(window, cx);
            return;
        }
        self.restore = window.focused(cx);
        self.open = true;
        self.query.update(cx, |field, cx| field.focus(window, cx));
        cx.notify();
    }

    /// Closes the sheet and returns focus to where it was.
    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        if let Some(handle) = self.restore.take() {
            window.focus(&handle, cx);
        }
        cx.notify();
    }

    /// The scrolling list of groups that match the filter.
    fn body(&self, m: &ModeTokens, cx: &App) -> impl IntoElement {
        let groups = filter_groups(&self.groups, &self.query.read(cx).value());
        let group_view = |(g, group): (usize, KeyGroup)| {
            let rows = group.rows.into_iter().enumerate().map(move |(r, row)| {
                let id = |k: usize| SharedString::from(format!("key-{g}-{r}-{k}"));
                let kbds: Vec<_> = (row.keys.iter().enumerate())
                    .filter_map(|(k, key)| Kbd::new(id(k), key))
                    .collect();
                (div().flex().justify_between().gap_3().py_1())
                    .child(row.label.clone().test_text(row.label))
                    .child(div().flex().gap_1().children(kbds))
            });
            let title = div().text_color(m.muted).child(group.title.clone());
            div().pb_3().child(title).children(rows)
        };
        let items = groups.into_iter().enumerate().map(group_view);
        div()
            .id("keys-body")
            .flex_1()
            .overflow_y_scroll()
            .children(items)
    }
}

impl Render for KeysSheet {
    /// Draws nothing while closed; open, the scrim with the 22 px card (NC68: solid, no blur).
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let (side, m) = tokens_of(cx);
        let card = div()
            .id("keys-card")
            .key_context(SHEET)
            .on_action(cx.listener(|this, _: &CloseKeys, window, cx| this.close(window, cx)))
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close(window, cx)))
            .flex()
            .flex_col()
            .gap_3()
            .w(px(520.))
            .max_h(px(560.))
            .p_4()
            .rounded(side.dims.sheet_r)
            .border_1()
            .border_color(m.sep_strong)
            .bg(m.card)
            .text_color(m.text)
            .shadow_lg()
            .child("Keyboard shortcuts".test_text("Keyboard shortcuts"))
            .child(TextField::new(
                "keys-filter",
                &self.query,
                "Filter shortcuts",
            ))
            .child(self.body(&m, cx));
        (div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center())
        .occlude()
        .bg(m.scrim)
        .child(card)
        .into_any_element()
    }
}

#[cfg(test)]
#[path = "keys_sheet_tests.rs"]
mod tests;
