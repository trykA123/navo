use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, Entity, Focusable as _, TestAppContext,
    VisualTestContext,
};
use rusty_core::Location;

use super::{
    FerriteWindow, StartOptions, fake, open_options, options_for, press, settle, two_panes,
};
use crate::commands::{Keymap, active};
use crate::shortcuts::{Edit, Saved, ShortcutsPage};
use crate::window_view::{KeymapHooks, KeymapNews, KeymapSender};

const ERROR: &str = "keymap.toml, line 2: unknown command `files.teleport`";
const SIZE_ON_ALT_F9: &str = "[[bind]]\ncommand = \"files.sort_by_size\"\nkeys = [\"alt-f9\"]\n";

type Entry = (&'static str, Option<String>, Vec<String>);

#[derive(Default)]
struct FakeKeymap {
    entries: Mutex<Vec<Entry>>,
    saves: Mutex<Vec<Vec<Edit>>>,
    failure: Mutex<Option<String>>,
}

impl FakeKeymap {
    fn text(&self) -> String {
        let entries = self.entries.lock().expect("entries");
        entries
            .iter()
            .map(|(command, context, keys)| {
                let keys: Vec<String> = keys.iter().map(|key| format!("\"{key}\"")).collect();
                let context = context
                    .as_ref()
                    .map_or_else(String::new, |context| format!("context = \"{context}\"\n"));
                format!(
                    "[[bind]]\ncommand = \"{command}\"\nkeys = [{}]\n{context}",
                    keys.join(", ")
                )
            })
            .collect()
    }

    fn apply(&self, edit: &Edit) {
        let mut entries = self.entries.lock().expect("entries");
        match edit {
            Edit::Set {
                command,
                context,
                keys,
            } => {
                let slot = entries
                    .iter_mut()
                    .find(|(name, at, _)| name == command && at == context);
                match slot {
                    Some(entry) => entry.2.clone_from(keys),
                    None => entries.push((command, context.clone(), keys.clone())),
                }
            }
            Edit::Reset { command } => entries.retain(|(name, _, _)| name != command),
        }
    }

    fn save(&self, edits: &[Edit]) -> Result<Saved, String> {
        self.saves.lock().expect("saves").push(edits.to_vec());
        if let Some(failure) = self.failure.lock().expect("failure").clone() {
            return Err(failure);
        }
        for edit in edits {
            self.apply(edit);
        }
        let overlaid = Keymap::builtin()
            .overlay(&self.text())
            .map_err(|error| error.to_string())?;
        Ok(Saved {
            keymap: overlaid.keymap,
            warnings: overlaid.warnings.iter().map(ToString::to_string).collect(),
        })
    }

    fn saves(&self) -> Vec<Vec<Edit>> {
        self.saves.lock().expect("saves").clone()
    }
}

pub(in crate::window_view) struct Rig {
    pub(in crate::window_view) window: AnyWindowHandle,
    pub(in crate::window_view) view: Entity<FerriteWindow>,
    sender: KeymapSender,
    keymap: Arc<FakeKeymap>,
}

fn applied(text: &str) -> KeymapNews {
    let overlaid = Keymap::builtin().overlay(text).expect("valid user keymap");
    KeymapNews::Applied {
        keymap: overlaid.keymap,
        warnings: Vec::new(),
    }
}

fn rig_with(cx: &mut TestAppContext, start: KeymapNews) -> Rig {
    let fs = fake();
    fs.add_file("home/b.txt", 9);
    fs.add_file("home/c.txt", 1);
    fs.add_file("home/keymap.toml", 40);
    let file = fs.location("home/keymap.toml");
    let state = two_panes(&fs);
    rig_in(cx, start, options_for(fs, state), file)
}

pub(in crate::window_view) fn rig_on(
    cx: &mut TestAppContext,
    options: StartOptions,
    file: Location,
) -> Rig {
    rig_in(cx, applied(""), options, file)
}

fn rig_in(
    cx: &mut TestAppContext,
    start: KeymapNews,
    options: StartOptions,
    file: Location,
) -> Rig {
    let keymap = Arc::new(FakeKeymap::default());
    let fake_keymap = keymap.clone();
    let (hooks, sender) = KeymapHooks::channel(Some(start));
    let hooks = hooks.with_file(
        Some(file),
        Arc::new(move |edits: &[Edit]| fake_keymap.save(edits)),
    );
    let options = StartOptions {
        keymap: hooks,
        ..options
    };
    let (window, view) = open_options(cx, options);
    Rig {
        window,
        view,
        sender,
        keymap,
    }
}

fn rig(cx: &mut TestAppContext) -> Rig {
    rig_with(cx, applied(""))
}

fn rig_from(cx: &mut TestAppContext, text: &str) -> Rig {
    let rig = rig_with(cx, applied(text));
    rig.keymap.entries.lock().expect("entries").push((
        "files.sort_by_size",
        None,
        vec!["alt-f9".to_owned()],
    ));
    rig
}

impl Rig {
    pub(in crate::window_view) fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    fn type_text(&self, cx: &mut TestAppContext, text: &str) {
        cx.update_window(self.window, |_, window, cx| window.input(text, cx))
            .expect("window");
        settle(cx, &self.view);
    }

    fn type_keys(&self, cx: &mut TestAppContext, text: &str) {
        for character in text.chars() {
            let key = match character {
                '+' => "shift-=->+".to_owned(),
                ' ' => "space".to_owned(),
                upper if upper.is_ascii_uppercase() => {
                    format!("shift-{}", upper.to_ascii_lowercase())
                }
                other => other.to_string(),
            };
            self.press(cx, &key);
        }
    }

    fn query(&self, cx: &mut TestAppContext) -> String {
        self.page(cx)
            .read_with(cx, |page, _| page.query().to_owned())
    }

    fn click(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) {
        let id = id.into();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.click(id, cx);
        })
        .expect("window");
        settle(cx, &self.view);
    }

    fn settle_motion(&self, cx: &mut TestAppContext) {
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
    }

    pub(in crate::window_view) fn open_page(&self, cx: &mut TestAppContext) {
        self.press(cx, "ctrl-,");
        self.settle_motion(cx);
        self.click(
            cx,
            ("settings-section", crate::settings::Section::ALL.len()),
        );
    }

    fn page(&self, cx: &mut TestAppContext) -> Entity<ShortcutsPage> {
        self.view
            .read_with(cx, |view, _| view.shortcuts_page())
            .expect("the shortcuts page exists")
    }

    pub(in crate::window_view) fn search(&self, cx: &mut TestAppContext, query: &str) {
        let page = self.page(cx);
        cx.update_window(self.window, |_, window, cx| {
            page.update(cx, |page, cx| page.set_query(query, window, cx));
        })
        .expect("window");
        settle(cx, &self.view);
    }

    fn visible(&self, cx: &mut TestAppContext) -> Vec<&'static str> {
        self.page(cx).read_with(cx, |page, _| page.visible())
    }

    pub(in crate::window_view) fn is_recording(&self, cx: &mut TestAppContext) -> bool {
        self.page(cx).read_with(cx, |page, _| page.is_recording())
    }

    fn settings_open(&self, cx: &mut TestAppContext) -> bool {
        self.view.read_with(cx, |view, _| view.settings.is_open())
    }

    pub(in crate::window_view) fn label(
        &self,
        cx: &mut TestAppContext,
        id: impl Into<ElementId>,
    ) -> Option<String> {
        let id = id.into();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(id)
                .filter(|found| found.visible())
                .and_then(|found| found.label().map(str::to_owned))
        })
        .expect("window")
    }

    fn shows(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find(id).is_some_and(|found| found.visible())
        })
        .expect("window")
    }

    fn has_key(&self, cx: &mut TestAppContext, command: &str, key: &str) -> bool {
        cx.read(|cx| active(cx).has_key(command, key))
    }

    fn applies(&self, cx: &mut TestAppContext) -> usize {
        self.view.read_with(cx, |view, _| view.keymap_applies())
    }

    fn files(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            let pane = state.pane(state.focused_pane()).expect("pane");
            pane.active()
                .listing()
                .map(|listing| {
                    listing
                        .entries()
                        .iter()
                        .map(|entry| entry.name.clone())
                        .filter(|name| name.ends_with(".txt"))
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    pub(in crate::window_view) fn record_on(
        &self,
        cx: &mut TestAppContext,
        command: &str,
        key: &str,
    ) {
        self.search(cx, command);
        self.press(cx, "enter");
        assert!(self.is_recording(cx), "recording {command}");
        self.press(cx, key);
    }
}

fn set(command: &'static str, keys: &[&str]) -> Edit {
    Edit::Set {
        command,
        context: None,
        keys: keys.iter().map(|&key| key.to_owned()).collect(),
    }
}

fn changed_mark(command: &str) -> ElementId {
    ElementId::Name(format!("shortcut-changed-{command}").into())
}

#[gpui_kit::test]
fn the_page_lists_every_command_and_its_search_has_focus(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    assert!(rig.shows(cx, "shortcuts-page"));
    assert_eq!(rig.visible(cx).len(), crate::commands::all().count());
    rig.type_text(cx, "files.sort_by_s");
    assert_eq!(rig.visible(cx), ["files.sort_by_size"]);
    rig.press(cx, "escape");
    assert!(!rig.settings_open(cx));
}

#[gpui_kit::test]
fn search_finds_a_label(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.search(cx, "sort by size");
    assert_eq!(rig.visible(cx), ["files.sort_by_size"]);
}

const CTRL_F5: [&str; 3] = [
    "files.sort_by_date",
    "elf.sort_by_address",
    "map.sort_by_address",
];

#[gpui_kit::test]
fn the_page_lists_the_split_and_close_pane_keys(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    for (typed, command) in [
        ("ctrl+alt+v", "layout.split_right"),
        ("ctrl+alt+b", "layout.split_below"),
        ("ctrl+alt+w", "layout.close_pane"),
    ] {
        rig.search(cx, typed);
        assert!(rig.visible(cx).contains(&command), "{typed}");
    }
}

#[gpui_kit::test]
fn search_finds_the_command_bound_to_a_typed_key(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.type_text(cx, "ctrl+f5");
    assert_eq!(rig.visible(cx), CTRL_F5);
}

#[gpui_kit::test]
fn keys_typed_as_on_a_keyboard_land_in_the_search(cx: &mut TestAppContext) {
    for typed in ["ctrl+f5", "Ctrl+F5", "f5 ctrl", "ctrl - f5", "F5+CTRL"] {
        let rig = rig(cx);
        rig.open_page(cx);
        rig.type_keys(cx, typed);
        assert_eq!(rig.query(cx), typed);
        assert_eq!(rig.visible(cx), CTRL_F5, "{typed}");
    }
}

#[gpui_kit::test]
fn a_plus_typed_in_the_search_leaves_the_pane_alone(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.type_keys(cx, "+-*");
    assert_eq!(rig.query(cx), "+-*");
    assert!(!rig.view.read_with(cx, |view, _| view.prompt.is_some()));
    let marked = rig.view.read_with(cx, |view, _| {
        view.browser.state().active_tab().selection().marked()
    });
    assert!(marked.is_empty());
}

#[gpui_kit::test]
fn a_recorded_key_is_saved_and_works_at_once(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.record_on(cx, "files.sort_by_size", "alt-f9");
    assert!(!rig.is_recording(cx));
    assert_eq!(
        rig.keymap.saves(),
        [vec![set("files.sort_by_size", &["alt-f9"])]]
    );
    assert!(rig.has_key(cx, "files.sort_by_size", "alt-f9"));
    assert!(rig.shows(cx, changed_mark("files.sort_by_size")));
    rig.press(cx, "escape");
    assert!(!rig.settings_open(cx));
    rig.press(cx, "alt-f9");
    let files = rig.files(cx);
    assert_eq!(files.get(1).map(String::as_str), Some("a.txt"), "{files:?}");
}

#[gpui_kit::test]
fn a_click_on_a_row_records_too(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.search(cx, "files.sort_by_size");
    rig.click(cx, ElementId::Name("shortcut-files.sort_by_size".into()));
    assert!(rig.is_recording(cx));
    rig.press(cx, "alt-f9");
    assert_eq!(
        rig.keymap.saves(),
        [vec![set("files.sort_by_size", &["alt-f9"])]]
    );
}

#[gpui_kit::test]
fn a_global_key_is_recorded_instead_of_run_and_asks_before_hiding_it(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.record_on(cx, "files.sort_by_size", "f10");
    let menu_open = rig.view.read_with(cx, |view, _| view.app_menu.is_some());
    assert!(!menu_open);
    assert_eq!(
        rig.label(cx, "shortcuts-prompt").as_deref(),
        Some("F10 is used by App menu — your key would hide it here. Replace?")
    );
    assert!(rig.keymap.saves().is_empty());
    rig.press(cx, "enter");
    assert_eq!(
        rig.keymap.saves(),
        [vec![set("files.sort_by_size", &["f10"])]]
    );
    assert!(rig.has_key(cx, "app_menu.toggle", "f10"));
    rig.press(cx, "escape");
    rig.press(cx, "f10");
    let menu_open = rig.view.read_with(cx, |view, _| view.app_menu.is_some());
    assert!(!menu_open);
    let files = rig.files(cx);
    assert_eq!(files.get(1).map(String::as_str), Some("a.txt"), "{files:?}");
}

#[gpui_kit::test]
fn a_conflict_asks_before_saving_and_replace_takes_the_key(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.record_on(cx, "files.sort_by_size", "ctrl-f5");
    assert_eq!(
        rig.label(cx, "shortcuts-prompt").as_deref(),
        Some("Ctrl+F5 is used by Sort by date — replace it?")
    );
    assert!(rig.keymap.saves().is_empty());
    rig.press(cx, "enter");
    assert_eq!(
        rig.keymap.saves(),
        [vec![set("files.sort_by_size", &["ctrl-f5"])]]
    );
    assert!(rig.has_key(cx, "files.sort_by_size", "ctrl-f5"));
    assert!(!rig.has_key(cx, "files.sort_by_date", "ctrl-f5"));
    assert!(!rig.shows(cx, "shortcuts-prompt"));
}

#[gpui_kit::test]
fn the_replace_button_saves_too(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.record_on(cx, "files.sort_by_size", "ctrl-f5");
    rig.click(cx, "shortcuts-replace");
    assert_eq!(rig.keymap.saves().len(), 1);
    assert!(rig.has_key(cx, "files.sort_by_size", "ctrl-f5"));
}

#[gpui_kit::test]
fn escape_on_a_conflict_saves_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.record_on(cx, "files.sort_by_size", "ctrl-f5");
    rig.press(cx, "escape");
    assert!(!rig.shows(cx, "shortcuts-prompt"));
    assert!(rig.settings_open(cx));
    assert!(rig.keymap.saves().is_empty());
    assert!(rig.has_key(cx, "files.sort_by_date", "ctrl-f5"));
}

fn move_focus(rig: &Rig, cx: &mut TestAppContext, to_search: bool) {
    let page = rig.page(cx);
    cx.update_window(rig.window, |_, window, cx| {
        if to_search {
            let search = page.read(cx).focus_handle(cx);
            window.focus(&search, cx);
        } else {
            window.blur(cx);
        }
        window.render_frame(cx);
    })
    .expect("window");
    settle(cx, &rig.view);
}

#[gpui_kit::test]
fn leaving_the_search_field_stops_recording(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let mut visual = VisualTestContext::from_window(rig.window, cx);
    visual.deactivate_window();
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    rig.open_page(cx);
    rig.search(cx, "files.sort_by_size");
    rig.press(cx, "enter");
    assert!(rig.is_recording(cx));
    move_focus(&rig, cx, false);
    assert!(!rig.is_recording(cx));
    move_focus(&rig, cx, true);
    rig.press(cx, "alt-f9");
    assert!(!rig.is_recording(cx));
    assert!(rig.keymap.saves().is_empty());
    assert!(!rig.has_key(cx, "files.sort_by_size", "alt-f9"));
    rig.click(cx, ElementId::Name("shortcut-files.sort_by_size".into()));
    assert!(rig.is_recording(cx));
    rig.press(cx, "alt-f9");
    assert!(rig.has_key(cx, "files.sort_by_size", "alt-f9"));
}

#[gpui_kit::test]
fn escape_while_recording_cancels_and_keeps_the_sheet_open(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.record_on(cx, "files.sort_by_size", "escape");
    assert!(!rig.is_recording(cx));
    assert!(rig.settings_open(cx));
    assert!(rig.keymap.saves().is_empty());
}

#[gpui_kit::test]
fn backspace_while_recording_unbinds_the_command(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.record_on(cx, "files.copy", "backspace");
    assert_eq!(rig.keymap.saves(), [vec![set("files.copy", &[])]]);
    let keys = cx.read(|cx| active(cx).keys_of("files.copy").count());
    assert_eq!(keys, 0);
}

#[gpui_kit::test]
fn reset_removes_the_user_entry_of_a_changed_row(cx: &mut TestAppContext) {
    let rig = rig_from(cx, SIZE_ON_ALT_F9);
    rig.open_page(cx);
    rig.search(cx, "files.sort_by_size");
    assert!(rig.shows(cx, changed_mark("files.sort_by_size")));
    assert!(!rig.shows(cx, changed_mark("files.sort_by_date")));
    rig.press(cx, "ctrl-r");
    assert_eq!(
        rig.keymap.saves(),
        [vec![Edit::Reset {
            command: "files.sort_by_size"
        }]]
    );
    assert!(!rig.has_key(cx, "files.sort_by_size", "alt-f9"));
    assert!(rig.has_key(cx, "files.sort_by_size", "ctrl-f6"));
    assert!(!rig.shows(cx, changed_mark("files.sort_by_size")));
}

#[gpui_kit::test]
fn an_unchanged_row_has_nothing_to_reset(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    rig.search(cx, "files.sort_by_size");
    rig.press(cx, "ctrl-r");
    assert!(rig.keymap.saves().is_empty());
}

#[gpui_kit::test]
fn an_invalid_file_shows_its_error_and_nothing_is_written(cx: &mut TestAppContext) {
    let rig = rig_with(cx, KeymapNews::Invalid(ERROR.to_owned()));
    rig.open_page(cx);
    let problem = rig.label(cx, "shortcuts-problem").unwrap_or_default();
    assert!(problem.contains(ERROR), "{problem}");
    rig.search(cx, "files.sort_by_size");
    rig.press(cx, "enter");
    assert!(!rig.is_recording(cx));
    rig.press(cx, "ctrl-r");
    assert!(rig.keymap.saves().is_empty());
    rig.click(cx, "shortcuts-open-file");
    assert!(rig.view.read_with(cx, |view, _| view.viewer.is_some()));
}

#[gpui_kit::test]
fn a_fixed_file_lifts_the_block(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    assert!(rig.sender.send(KeymapNews::Invalid(ERROR.to_owned())));
    cx.run_until_parked();
    assert!(rig.shows(cx, "shortcuts-problem"));
    assert!(rig.sender.send(applied("")));
    cx.run_until_parked();
    assert!(!rig.shows(cx, "shortcuts-problem"));
    rig.record_on(cx, "files.sort_by_size", "alt-f9");
    assert_eq!(rig.keymap.saves().len(), 1);
}

#[gpui_kit::test]
fn a_failed_save_keeps_the_change_and_shows_the_error(cx: &mut TestAppContext) {
    let rig = rig(cx);
    *rig.keymap.failure.lock().expect("failure") = Some("disk full".to_owned());
    rig.open_page(cx);
    rig.record_on(cx, "files.sort_by_size", "alt-f9");
    let error = rig.label(cx, "shortcuts-error").unwrap_or_default();
    assert!(error.contains("disk full"), "{error}");
    assert!(!rig.has_key(cx, "files.sort_by_size", "alt-f9"));
    *rig.keymap.failure.lock().expect("failure") = None;
    rig.press(cx, "enter");
    assert_eq!(rig.keymap.saves().len(), 2);
    assert!(rig.has_key(cx, "files.sort_by_size", "alt-f9"));
    assert!(!rig.shows(cx, "shortcuts-error"));
}

#[gpui_kit::test]
fn a_failed_save_survives_closing_the_sheet(cx: &mut TestAppContext) {
    let rig = rig(cx);
    *rig.keymap.failure.lock().expect("failure") = Some("disk full".to_owned());
    rig.open_page(cx);
    rig.record_on(cx, "files.sort_by_size", "alt-f9");
    rig.press(cx, "escape");
    rig.press(cx, "ctrl-,");
    rig.settle_motion(cx);
    let error = rig.label(cx, "shortcuts-error").unwrap_or_default();
    assert!(error.contains("disk full"), "{error}");
}

#[gpui_kit::test]
fn the_watcher_echo_of_a_save_does_not_rebind_again(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_page(cx);
    let before = rig.applies(cx);
    rig.record_on(cx, "files.sort_by_size", "alt-f9");
    assert_eq!(rig.applies(cx), before + 1);
    let echo = applied(&rig.keymap.text());
    assert!(rig.sender.send(echo));
    cx.run_until_parked();
    assert_eq!(rig.applies(cx), before + 1);
    assert!(rig.sender.send(applied("")));
    cx.run_until_parked();
    assert_eq!(rig.applies(cx), before + 2);
}
