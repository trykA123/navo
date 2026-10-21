use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Focusable, TestAppContext};
use rusty_core::{AppState, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FakeFs, FsEntry};

use super::{open_options, options_for, press, settle};
use crate::browser::{NamedPreset, RenameHooks, RenamePresetList};
use crate::window_view::FerriteWindow;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
}

fn files_fs(fs: FakeFs) -> Arc<FakeFs> {
    fs.add_folder("home/docs");
    for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        fs.add_file(&format!("home/{name}"), 1);
    }
    Arc::new(fs)
}

fn rig_on(cx: &mut TestAppContext, fs: Arc<FakeFs>) -> Rig {
    let state = AppState::new(fs.location("home"));
    let (window, view) = open_options(cx, options_for(fs, state));
    Rig { window, view }
}

fn rig(cx: &mut TestAppContext) -> Rig {
    rig_on(cx, files_fs(FakeFs::new()))
}

fn marked_three(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx);
    rig.press(cx, &["down", "shift-down", "shift-down"]);
    rig
}

fn opened(cx: &mut TestAppContext) -> Rig {
    let rig = marked_three(cx);
    rig.press(cx, &["ctrl-m"]);
    rig
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, keys: &[&str]) {
        for key in keys {
            press(cx, self.window, &self.view, key);
            self.wait(cx);
        }
    }

    fn wait(&self, cx: &mut TestAppContext) {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        settle(cx, &self.view);
    }

    fn type_text(&self, cx: &mut TestAppContext, text: &str) {
        cx.update_window(self.window, |_, window, cx| window.input(text, cx))
            .expect("window");
        self.wait(cx);
    }

    fn rows(&self, cx: &mut TestAppContext) -> Vec<(String, String)> {
        self.view
            .read_with(cx, |view, _| view.multi_rename_rows())
            .unwrap_or_default()
    }

    fn new_names(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.rows(cx).into_iter().map(|(_, new)| new).collect()
    }

    fn is_open(&self, cx: &mut TestAppContext) -> bool {
        self.view
            .read_with(cx, |view, _| view.multi_rename_inputs().is_some())
    }

    fn listing(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.view.read_with(cx, |view, _| {
            let tab = view.browser.state().active_tab();
            tab.listing()
                .map(|listing| listing.entries().iter().map(|e| e.name.clone()).collect())
                .unwrap_or_default()
        })
    }

    fn cursor_name(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            let tab = view.browser.state().active_tab();
            let listing = tab.listing()?;
            listing
                .get(tab.view().cursor)
                .map(|entry| entry.name.clone())
        })
    }

    fn preview_cursor(&self, cx: &mut TestAppContext) -> Option<usize> {
        self.view
            .read_with(cx, |view, cx| view.multi_rename_cursor(cx))
    }

    fn pane_has_focus(&self, cx: &mut TestAppContext) -> bool {
        let pane = self
            .view
            .read_with(cx, |view, _| view.browser.state().focused_pane());
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            let handle = view.update(cx, |view, cx| view.handle(pane, cx));
            handle.is_focused(window)
        })
        .expect("window")
    }

    fn focus_find(&self, cx: &mut TestAppContext) {
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            let inputs = view.read(cx).multi_rename_inputs().expect("open");
            let handle = inputs.find.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        })
        .expect("window");
        self.wait(cx);
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

#[gpui_kit::test]
fn ctrl_m_with_three_marks_shows_three_rows(cx: &mut TestAppContext) {
    let rig = opened(cx);
    let rows = rig.rows(cx);
    let old: Vec<&str> = rows.iter().map(|(old, _)| old.as_str()).collect();
    assert_eq!(old, ["a.txt", "b.txt", "c.txt"]);
    assert_eq!(rig.new_names(cx), names(&["a.txt", "b.txt", "c.txt"]));
}

#[gpui_kit::test]
fn a_counter_mask_updates_the_preview(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "[N]_[C2]");
    assert_eq!(rig.new_names(cx), names(&["a_2.txt", "b_3.txt", "c_4.txt"]));
}

#[gpui_kit::test]
fn enter_renames_and_the_cursor_lands_on_the_first_new_name(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "[N]_[C2]");
    rig.press(cx, &["enter"]);
    assert!(!rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a_2.txt", "b_3.txt", "c_4.txt", "d.txt"])
    );
    assert_eq!(rig.cursor_name(cx).as_deref(), Some("a_2.txt"));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn enter_types_then_runs_before_the_preview_catches_up(cx: &mut TestAppContext) {
    let rig = opened(cx);
    cx.update_window(rig.window, |_, window, cx| {
        window.input("x[N]", cx);
        window.press("enter", cx);
    })
    .expect("window");
    rig.wait(cx);
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "d.txt", "xa.txt", "xb.txt", "xc.txt"])
    );
}

#[gpui_kit::test]
fn enter_with_a_conflict_renames_nothing_and_moves_to_it(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.focus_find(cx);
    rig.type_text(cx, "b");
    rig.press(cx, &["tab"]);
    rig.type_text(cx, "d");
    assert_eq!(rig.new_names(cx), names(&["a.txt", "d.txt", "c.txt"]));
    rig.press(cx, &["enter"]);
    assert!(rig.is_open(cx));
    assert_eq!(rig.preview_cursor(cx), Some(1));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "d.txt"])
    );
}

#[gpui_kit::test]
fn escape_closes_with_no_change_and_returns_focus(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "[N]_[C2]");
    rig.press(cx, &["escape"]);
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "d.txt"])
    );
}

#[gpui_kit::test]
fn delete_in_the_preview_excludes_the_row(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "[N]_[C2]");
    rig.press(cx, &["ctrl-down", "delete"]);
    assert_eq!(rig.new_names(cx), names(&["a.txt", "b_2.txt", "c_3.txt"]));
}

#[gpui_kit::test]
fn a_bad_token_shows_an_error_and_blocks_enter(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "[Q]");
    let errors = rig
        .view
        .read_with(cx, |view, _| view.multi_rename_errors())
        .expect("open");
    assert!(errors.name_mask.is_some());
    rig.press(cx, &["enter"]);
    assert!(rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "d.txt"])
    );
}

#[gpui_kit::test]
fn f2_with_two_marks_opens_the_tool(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["down", "shift-down", "f2"]);
    assert_eq!(rig.rows(cx).len(), 2);
}

#[gpui_kit::test]
fn the_token_menu_inserts_at_the_caret(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "[N]_");
    rig.press(cx, &["ctrl-space"]);
    rig.type_text(cx, "counter");
    rig.press(cx, &["enter"]);
    assert_eq!(rig.new_names(cx), names(&["a_1.txt", "b_2.txt", "c_3.txt"]));
}

fn memory_hooks(store: &Arc<Mutex<Vec<NamedPreset>>>) -> RenameHooks {
    let (list, save, delete) = (store.clone(), store.clone(), store.clone());
    RenameHooks {
        last: Arc::new(|| Ok(None)),
        undo_log: Arc::new(|_| Err("none".to_owned())),
        presets: Arc::new(move || {
            let named = list.lock().map(|named| named.clone()).unwrap_or_default();
            Ok(RenamePresetList {
                named: named.into_iter().filter(|p| !p.name.is_empty()).collect(),
                last_used: None,
            })
        }),
        save: Arc::new(move |name, rules| {
            let mut named = save.lock().map_err(|e| e.to_string())?;
            named.retain(|preset| preset.name != name);
            named.push(NamedPreset {
                name: name.to_owned(),
                rules: rules.to_owned(),
            });
            Ok(())
        }),
        delete: Arc::new(move |name| {
            let mut named = delete.lock().map_err(|e| e.to_string())?;
            named.retain(|preset| preset.name != name);
            Ok(())
        }),
    }
}

#[gpui_kit::test]
fn a_preset_saved_with_ctrl_s_loads_with_ctrl_o(cx: &mut TestAppContext) {
    let rig = marked_three(cx);
    let store = Arc::new(Mutex::new(Vec::new()));
    rig.view.update(cx, |view, _| {
        view.browser.set_rename_hooks(Some(memory_hooks(&store)));
    });
    rig.press(cx, &["ctrl-m"]);
    rig.type_text(cx, "new_[C]");
    rig.press(cx, &["ctrl-s"]);
    rig.type_text(cx, "mine");
    rig.press(cx, &["enter", "ctrl-r"]);
    assert_eq!(rig.new_names(cx), names(&["a.txt", "b.txt", "c.txt"]));
    rig.press(cx, &["ctrl-o", "enter"]);
    assert_eq!(
        rig.new_names(cx),
        names(&["new_1.txt", "new_2.txt", "new_3.txt"])
    );
}

#[gpui_kit::test]
fn page_down_from_a_field_moves_the_preview(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.press(cx, &["pagedown", "pagedown"]);
    assert_eq!(rig.preview_cursor(cx), Some(2));
}

#[gpui_kit::test]
fn alt_down_walks_the_problems_and_tab_reaches_the_preview(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "same");
    rig.press(cx, &["alt-down"]);
    assert_eq!(rig.preview_cursor(cx), Some(0));
    rig.press(cx, &["alt-down"]);
    assert_eq!(rig.preview_cursor(cx), Some(1));
    rig.press(cx, &["ctrl-up"]);
    rig.press(cx, &["shift-tab"]);
    rig.press(cx, &["delete"]);
    assert_eq!(rig.new_names(cx)[1], "b.txt");
}

fn typed_then_enter(rig: &Rig, cx: &mut TestAppContext, text: &str) {
    cx.update_window(rig.window, |_, window, cx| {
        window.input(text, cx);
        window.press("enter", cx);
    })
    .expect("window");
}

#[gpui_kit::test]
fn an_enter_waiting_for_the_preview_does_not_run_later_edits(cx: &mut TestAppContext) {
    let rig = opened(cx);
    typed_then_enter(&rig, cx, "x[N]");
    cx.update_window(rig.window, |_, window, cx| window.input("y", cx))
        .expect("window");
    rig.wait(cx);
    assert!(rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "d.txt"])
    );
    rig.press(cx, &["enter"]);
    assert!(!rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "d.txt", "xay.txt", "xby.txt", "xcy.txt"])
    );
}

#[gpui_kit::test]
fn an_enter_waiting_for_the_preview_does_not_run_after_a_row_is_excluded(cx: &mut TestAppContext) {
    let rig = opened(cx);
    typed_then_enter(&rig, cx, "x[N]");
    cx.update_window(rig.window, |_, window, cx| {
        window.press("ctrl-down", cx);
        window.press("delete", cx);
    })
    .expect("window");
    rig.wait(cx);
    assert!(rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "d.txt"])
    );
}

#[gpui_kit::test]
fn enter_after_deleting_a_row_leaves_that_file_alone(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.type_text(cx, "[N]_[C2]");
    rig.press(cx, &["ctrl-down", "delete", "enter"]);
    assert!(!rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b_2.txt", "c_3.txt", "d.txt"])
    );
}

fn watched_rig(cx: &mut TestAppContext) -> (Rig, Arc<FakeFs>) {
    let fs = files_fs(FakeFs::new().with_watch());
    let rig = rig_on(cx, fs.clone());
    rig.press(cx, &["down", "shift-down", "shift-down", "ctrl-m"]);
    (rig, fs)
}

fn wait_for_listing(rig: &Rig, cx: &mut TestAppContext, name: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !rig.listing(cx).iter().any(|entry| entry == name) {
        settle(cx, &rig.view);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(rig.listing(cx).iter().any(|entry| entry == name));
}

#[gpui_kit::test]
fn a_file_that_appears_after_opening_stops_enter_and_the_preview_shows_the_clash(
    cx: &mut TestAppContext,
) {
    let (rig, fs) = watched_rig(cx);
    rig.focus_find(cx);
    rig.type_text(cx, "b");
    rig.press(cx, &["tab"]);
    rig.type_text(cx, "e");
    assert_eq!(rig.new_names(cx), names(&["a.txt", "e.txt", "c.txt"]));
    fs.add_file("home/e.txt", 1);
    wait_for_listing(&rig, cx, "e.txt");
    rig.press(cx, &["enter"]);
    assert!(rig.is_open(cx));
    assert_eq!(rig.preview_cursor(cx), None);
    rig.press(cx, &["enter"]);
    assert!(rig.is_open(cx));
    assert_eq!(rig.preview_cursor(cx), Some(1));
    assert!(rig.listing(cx).iter().any(|entry| entry == "b.txt"));
}

fn hidden_entry(name: &str) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 1,
        modified_ms: None,
        attributes: Attributes {
            hidden: true,
            ..Attributes::default()
        },
    }
}

#[gpui_kit::test]
fn a_clash_with_a_hidden_file_is_shown_and_blocks_enter(cx: &mut TestAppContext) {
    let fs = FakeFs::new();
    fs.add_entry("home", hidden_entry("e.txt"));
    let rig = rig_on(cx, files_fs(fs));
    rig.view
        .update(cx, |view, _| view.browser.set_show_hidden(false));
    settle(cx, &rig.view);
    assert!(!rig.listing(cx).iter().any(|entry| entry == "e.txt"));
    rig.press(cx, &["down", "shift-down", "shift-down", "ctrl-m"]);
    rig.focus_find(cx);
    rig.type_text(cx, "b");
    rig.press(cx, &["tab"]);
    rig.type_text(cx, "e");
    rig.press(cx, &["enter"]);
    assert!(rig.is_open(cx));
    assert_eq!(rig.preview_cursor(cx), Some(1));
    assert!(rig.listing(cx).iter().any(|entry| entry == "b.txt"));
}

#[gpui_kit::test]
fn the_dot_dot_row_is_never_a_source(cx: &mut TestAppContext) {
    let fs = FakeFs::new();
    fs.add_file("home/docs/x.txt", 1);
    fs.add_entry(
        "home/docs",
        FsEntry {
            name: ProviderPath::from_text(".."),
            kind: EntryKind::Folder,
            size: 0,
            modified_ms: None,
            attributes: Attributes::default(),
        },
    );
    let rig = rig_on(cx, Arc::new(fs));
    rig.press(cx, &["enter"]);
    assert!(rig.listing(cx).iter().any(|entry| entry == ".."));
    rig.press(cx, &["home", "shift-down", "shift-down", "ctrl-m"]);
    let rows = rig.rows(cx);
    let old: Vec<&str> = rows.iter().map(|(old, _)| old.as_str()).collect();
    assert!(!old.contains(&".."));
    assert_eq!(rig.cursor_name(cx).as_deref(), Some("x.txt"));
}

#[gpui_kit::test]
fn ctrl_k_does_not_open_over_the_tool(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.press(cx, &["ctrl-k"]);
    let open = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find("command-center").is_some()
        })
        .expect("window");
    assert!(!open);
    assert!(rig.is_open(cx));
}

#[gpui_kit::test]
fn enter_with_nothing_to_rename_keeps_the_tool_open(cx: &mut TestAppContext) {
    let rig = opened(cx);
    rig.press(cx, &["enter"]);
    assert!(rig.is_open(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "d.txt"])
    );
}
