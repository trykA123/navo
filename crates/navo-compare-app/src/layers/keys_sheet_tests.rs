//! Tests of the Keys sheet: key groups, the live filter, the keys that open it and focus
//! restore on Escape.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::test_support::texts;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, FocusHandle, Modifiers, Point, TestAppContext, VisualTestContext, px,
};
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::Mode;

use super::super::keys_sheet_compare::{compare_groups, groups_from, translated};
use super::*;
use crate::test_window::open;
use crate::theme::apply_sidewise;

/// A stand-in shell: the `Shell` context over a focusable body, and the sheet.
struct Root {
    keys: Entity<KeysSheet>,
    body: FocusHandle,
    /// Clicks that reached the body under the sheet.
    clicks: Rc<Cell<u32>>,
}

impl Render for Root {
    /// Wires the actions the way the shell will and mounts the sheet beside the body.
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = Rc::clone(&self.clicks);
        let body = (div().id("body").track_focus(&self.body).flex_1())
            .on_click(move |_, _, _| clicks.set(clicks.get() + 1));
        let root = div().key_context("Shell").size_full().flex().child(body);
        wire(root, &self.keys).child(self.keys.clone())
    }
}

/// A window holding `Root` with the body focused and the keys bound.
fn root(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Root>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(PaletteId::Violet, Mode::Light, cx);
        bind_keys(cx);
    });
    let (window, root) = open(cx, |window, cx| {
        let root = Root {
            keys: cx.new(|cx| KeysSheet::new(window, cx)),
            body: cx.focus_handle(),
            clicks: Rc::default(),
        };
        window.focus(&root.body, cx);
        root
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    (window, root)
}

/// The sheet of a root.
fn sheet(cx: &mut TestAppContext, root: &Entity<Root>) -> Entity<KeysSheet> {
    root.read_with(cx, |root, _| root.keys.clone())
}

/// The labels of the sheet's rows that match `query`, in order.
fn labels(query: &str) -> Vec<SharedString> {
    let groups = filter_groups(&key_groups(), query);
    groups
        .iter()
        .flat_map(|group| group.rows.iter().map(|row| row.label.clone()))
        .collect()
}

/// The groups hold the keys that exist now, and none of the dropped ones.
#[gpui_kit::test]
fn the_groups_list_the_keys_that_exist_now(_cx: &mut TestAppContext) {
    let groups = key_groups();
    let titles: Vec<&str> = groups.iter().map(|group| group.title.as_ref()).collect();
    assert_eq!(titles[0], "Window", "Sidewise keys come first");
    assert!(
        ["Compare tab", "Folders", "Large"]
            .iter()
            .all(|title| titles.contains(title)),
        "{titles:?}"
    );
    let keys: Vec<&str> = groups
        .iter()
        .flat_map(|group| {
            group
                .rows
                .iter()
                .flat_map(|row| row.keys.iter().map(String::as_str))
        })
        .collect();
    for want in ["?", "f1", "ctrl-t", "f6", "alt-down", "5", "enter", "left"] {
        assert!(keys.contains(&want), "missing {want}");
    }
    for dropped in ["t", "shift-t", "6"] {
        assert!(!keys.contains(&dropped), "{dropped} must not be listed");
    }
}

/// Every compare binding that has a label shows as a row, with a translated label.
#[gpui_kit::test]
fn the_keys_sheet_lists_every_compare_binding(_cx: &mut TestAppContext) {
    use sidewise_ui::compare_keys::{COMPARE_LABELS, compare_keys};
    let keys = compare_keys().expect("the compare key file parses");
    let labelled = keys
        .iter()
        .filter(|key| COMPARE_LABELS.iter().any(|(name, _)| *name == key.command))
        .count();
    assert_eq!(labelled, keys.len(), "every bound command has a label");
    let shown = (keys.iter())
        .filter_map(|key| COMPARE_LABELS.iter().find(|(name, _)| *name == key.command))
        .filter(|(_, id)| translated(id).is_some())
        .count();
    let rows: Vec<KeyRow> = (compare_groups().into_iter())
        .flat_map(|group| group.rows)
        .collect();
    assert_eq!(rows.len(), shown, "rows without Fluent text are skipped");
    assert!(rows.len() >= 10, "{} rows", rows.len());
    let next = sidewise_l10n::tr("cmd-compare-next-difference");
    assert!(!next.is_empty() && next != "cmd-compare-next-difference");
    assert!(rows.iter().any(|row| row.label == next));
    let skipped = groups_from(&keys, &[], translated);
    assert!(skipped.is_empty(), "unlabelled commands are skipped");
}

/// Group titles are readable names, never a raw key context such as "CompareOutline".
#[gpui_kit::test]
fn compare_group_titles_are_readable_names(_cx: &mut TestAppContext) {
    let titles: Vec<String> = (compare_groups().into_iter())
        .map(|group| group.title.to_string())
        .collect();
    assert!(
        titles.iter().any(|title| title == "Find results"),
        "{titles:?}"
    );
    let raw = |title: &String| title.starts_with("Compare") && title != "Compare";
    assert!(!titles.iter().any(raw), "{titles:?}");
}

/// Every id in the compare label table has a catalog message, so no row is skipped.
#[gpui_kit::test]
fn every_compare_label_id_has_a_catalog_message(_cx: &mut TestAppContext) {
    use sidewise_ui::compare_keys::COMPARE_LABELS;
    let missing: Vec<&str> = (COMPARE_LABELS.iter())
        .map(|(_, id)| *id)
        .filter(|id| translated(id).is_none())
        .collect();
    assert!(missing.is_empty(), "no catalog message for {missing:?}");
}

/// The filter matches labels and shown keys, case-insensitively, and drops empty groups.
#[gpui_kit::test]
fn the_filter_matches_labels_and_keys(_cx: &mut TestAppContext) {
    assert_eq!(
        labels("NEW TAB"),
        ["New tab", "New tab"],
        "label, any case: Window and Compare tab"
    );
    assert!(
        labels("ctrl+w").iter().any(|label| label.contains("Close")),
        "shown key"
    );
    assert!(labels("no such command").is_empty());
    let groups = filter_groups(&key_groups(), "close tab");
    assert_eq!(groups.len(), 1, "groups without a match are dropped");
}

/// `?` and F1 open the sheet and Escape closes it and gives focus back to the body.
#[gpui_kit::test]
fn keys_open_the_sheet_and_escape_restores_focus(cx: &mut TestAppContext) {
    let (window, root_view) = root(cx);
    let keys = sheet(cx, &root_view);
    for key in ["?", "f1"] {
        cx.simulate_keystrokes(window, key);
        assert!(
            keys.read_with(cx, |sheet, _| sheet.is_open()),
            "{key} opens the sheet"
        );
        cx.simulate_keystrokes(window, "escape");
        assert!(
            !keys.read_with(cx, |sheet, _| sheet.is_open()),
            "escape closes after {key}"
        );
        let back = cx
            .update_window(window, |_, window, cx| {
                root_view.read(cx).body.is_focused(window)
            })
            .expect("window");
        assert!(back, "focus returns to the body after {key}");
    }
}

/// Typing in the filter field is text: a `?` does not close the sheet and lands in the field.
#[gpui_kit::test]
fn a_typed_question_mark_stays_text_in_the_filter(cx: &mut TestAppContext) {
    let (window, root_view) = root(cx);
    let keys = sheet(cx, &root_view);
    cx.simulate_keystrokes(window, "f1");
    cx.simulate_input(window, "?");
    assert!(
        keys.read_with(cx, |sheet, _| sheet.is_open()),
        "a typed ? stays text"
    );
    let typed = keys.read_with(cx, |sheet, cx| sheet.query.read(cx).value().to_string());
    assert_eq!(typed, "?", "the field holds the typed text");
}

/// The open sheet draws its title and a key row as visible text.
#[gpui_kit::test]
fn the_open_sheet_draws(cx: &mut TestAppContext) {
    let (window, root_view) = root(cx);
    cx.simulate_keystrokes(window, "f1");
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    assert!(sheet(cx, &root_view).read_with(cx, |sheet, _| sheet.is_open()));
    let seen = cx
        .update_window(window, |_, window, _| texts(window))
        .expect("window");
    for want in ["Keyboard shortcuts", "New tab"] {
        let found = seen.iter().find(|text| text.text() == want);
        assert!(found.is_some_and(|text| text.visible()), "{want} is drawn");
    }
}

/// A click on the scrim closes the sheet and never reaches the body under it.
#[gpui_kit::test]
fn a_click_on_the_scrim_closes_without_reaching_the_body(cx: &mut TestAppContext) {
    let (window, root_view) = root(cx);
    let corner = Point::new(px(10.), px(10.));
    let click = |cx: &mut TestAppContext| {
        VisualTestContext::from_window(window, cx).simulate_click(corner, Modifiers::none());
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .expect("window");
    };
    let clicks = |cx: &mut TestAppContext| root_view.read_with(cx, |root, _| root.clicks.get());
    click(cx);
    assert_eq!(clicks(cx), 1, "with the sheet closed the body is clicked");
    cx.simulate_keystrokes(window, "f1");
    click(cx);
    assert!(!sheet(cx, &root_view).read_with(cx, |sheet, _| sheet.is_open()));
    assert_eq!(clicks(cx), 1, "the scrim occludes the body");
}
