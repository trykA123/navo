use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use navo_core::Location;

use super::undo::{Rig, files};
use super::{open_options, options_for, press, viewer_open, viewer_window};
use crate::clip_palette::{
    ClipCounts, ClipId, ClipMode, ClipResults, ClipboardHooks, How, NoClipboard, SavedDraft, Scope,
    SecretText,
};
use crate::commands::entries_with;
use crate::window_view::clipboard_palette::ClipboardPanel;
use crate::window_view::keys::{
    ClearClipboardHistory, DeleteClipboardData, OpenClipboard, PauseClipboard,
};

const COUNTS: ClipCounts = ClipCounts {
    history: 12,
    saved: 3,
    notes: 1,
};

#[derive(Default)]
struct Spy {
    cleared: AtomicU32,
    wiped: AtomicU32,
    /// Offers no modes, as a history that is turned off.
    idle: bool,
    log: Mutex<Vec<&'static str>>,
}

fn off<T>() -> Result<T, String> {
    Err("off".to_owned())
}

impl ClipboardHooks for Spy {
    fn counts(&self) -> Option<ClipCounts> {
        Some(COUNTS)
    }
    fn modes(&self) -> Vec<ClipMode> {
        if self.idle {
            Vec::new()
        } else {
            vec![ClipMode::All]
        }
    }
    fn search(&self, _: &str, _: ClipMode, _: Option<&Location>) -> ClipResults {
        ClipResults::default()
    }
    fn copy(&self, _: ClipId, _: How) -> Result<(), String> {
        off()
    }
    fn delete(&self, _: ClipId) -> Result<(), String> {
        off()
    }
    fn clear_history(&self) -> Result<(), String> {
        self.cleared.fetch_add(1, Ordering::SeqCst);
        self.log.lock().expect("log").push("clear");
        Ok(())
    }
    fn delete_all(&self) -> Result<(), String> {
        self.wiped.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn set_recording(&self, _: bool) -> Result<(), String> {
        off()
    }
    fn subscribe(&self, _: Box<dyn Fn() + Send + Sync>) {}
    fn save_from_history(&self, _: ClipId) -> Result<ClipId, String> {
        off()
    }
    fn save(&self, _: SavedDraft) -> Result<ClipId, String> {
        off()
    }
    fn note_for(&self, _: &Location) -> Option<String> {
        None
    }
    fn set_note(&self, _: Scope, _: String) -> Result<(), String> {
        off()
    }
    fn reveal(&self, _: ClipId) -> Result<SecretText, String> {
        off()
    }
    fn copy_user(&self, _: ClipId) -> Result<(), String> {
        off()
    }
}

fn rig(cx: &mut TestAppContext, hooks: Option<Arc<dyn ClipboardHooks>>) -> Rig {
    let fs = files();
    let mut options = options_for(fs.clone(), navo_core::AppState::new(fs.location("home")));
    options.clipboard = hooks;
    let (window, view) = open_options(cx, options);
    Rig { window, view }
}

fn dispatch(cx: &mut TestAppContext, rig: &Rig, action: impl gpui_kit::Action) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(action), cx);
    })
    .expect("window");
    cx.run_until_parked();
}

fn state(cx: &mut TestAppContext, rig: &Rig) -> (bool, bool) {
    rig.view
        .read_with(cx, |view, _| (view.clip.is_open(), view.clip.asking()))
}

fn answer(cx: &mut TestAppContext, rig: &Rig, confirmed: bool) {
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |view, cx| {
            view.answer_clipboard_ask(confirmed, window, cx)
        });
    })
    .expect("window");
    cx.run_until_parked();
}

fn spy() -> (Arc<Spy>, Option<Arc<dyn ClipboardHooks>>) {
    let spy = Arc::new(Spy::default());
    (spy.clone(), Some(spy))
}

#[test]
fn the_four_commands_are_hidden_from_the_palette_without_hooks() {
    let names = ["open", "pause", "clear_history", "delete_all"].map(|n| format!("clipboard.{n}"));
    for has in [true, false] {
        for name in &names {
            let entry = entries_with(has)
                .into_iter()
                .find(|entry| entry.spec.name == *name)
                .expect("registered");
            assert_eq!((entry.enabled, entry.in_palette), (has, true));
        }
    }
}

#[gpui_kit::test]
fn no_clipboard_disables_the_commands_and_ctrl_shift_v_does_nothing(cx: &mut TestAppContext) {
    for hooks in [None, Some(Arc::new(NoClipboard) as Arc<dyn ClipboardHooks>)] {
        let rig = rig(cx, hooks);
        assert!(
            !rig.view
                .read_with(cx, |view, _| view.clip.hooks().is_some())
        );
        press(cx, rig.window, &rig.view, "ctrl-shift-v");
        dispatch(cx, &rig, ClearClipboardHistory);
        assert_eq!(state(cx, &rig), (false, false));
    }
}

#[gpui_kit::test]
fn a_history_that_is_off_hides_the_commands_but_the_palette_still_opens(cx: &mut TestAppContext) {
    let idle = Arc::new(Spy {
        idle: true,
        ..Spy::default()
    });
    let rig = rig(cx, Some(idle.clone()));
    let live = rig.view.read_with(cx, |view, _| view.clip.live().is_some());
    assert!(!live);
    let shown = rig.view.read_with(cx, |view, _| {
        view.command_entries()
            .iter()
            .filter(|entry| entry.spec.name.starts_with("clipboard.") && entry.enabled)
            .count()
    });
    assert_eq!(shown, 0);
    dispatch(cx, &rig, ClearClipboardHistory);
    dispatch(cx, &rig, DeleteClipboardData);
    assert_eq!(state(cx, &rig), (false, false));
    press(cx, rig.window, &rig.view, "ctrl-shift-v");
    assert_eq!(state(cx, &rig), (true, false));
    assert_eq!(idle.cleared.load(Ordering::SeqCst), 0);
}

#[gpui_kit::test]
fn ctrl_shift_v_opens_the_palette_from_the_file_list_and_escape_closes_it(cx: &mut TestAppContext) {
    let (_, hooks) = spy();
    let rig = rig(cx, hooks);
    assert!(
        rig.view
            .read_with(cx, |view, _| view.clip.hooks().is_some())
    );
    press(cx, rig.window, &rig.view, "ctrl-shift-v");
    assert_eq!(state(cx, &rig), (true, false));
    press(cx, rig.window, &rig.view, "escape");
    assert_eq!(state(cx, &rig), (false, false));
}

#[gpui_kit::test]
fn the_open_command_opens_the_palette(cx: &mut TestAppContext) {
    let (_, hooks) = spy();
    let rig = rig(cx, hooks);
    dispatch(cx, &rig, OpenClipboard);
    assert_eq!(state(cx, &rig), (true, false));
}

#[gpui_kit::test]
fn ctrl_shift_v_leaves_an_open_viewer_alone(cx: &mut TestAppContext) {
    let (window, view, _) = viewer_window(cx, Ok("hello\n"), false);
    let (_, hooks) = spy();
    view.update(cx, |view, _| view.clip = ClipboardPanel::new(hooks));
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "ctrl-shift-v");
    assert!(viewer_open(cx, &view));
    assert!(!view.read_with(cx, |view, _| view.clip.is_open()));
}

#[gpui_kit::test]
fn clear_history_asks_with_counts_and_runs_only_when_confirmed(cx: &mut TestAppContext) {
    let (spy, hooks) = spy();
    let rig = rig(cx, hooks);
    dispatch(cx, &rig, ClearClipboardHistory);
    assert_eq!(state(cx, &rig), (false, true));
    press(cx, rig.window, &rig.view, "escape");
    assert_eq!(state(cx, &rig), (false, false));
    assert_eq!(spy.cleared.load(Ordering::SeqCst), 0);
    dispatch(cx, &rig, ClearClipboardHistory);
    answer(cx, &rig, true);
    assert_eq!(spy.cleared.load(Ordering::SeqCst), 1);
    assert_eq!(spy.wiped.load(Ordering::SeqCst), 0);
}

#[gpui_kit::test]
fn delete_all_asks_and_confirming_wipes(cx: &mut TestAppContext) {
    let (spy, hooks) = spy();
    let rig = rig(cx, hooks);
    dispatch(cx, &rig, DeleteClipboardData);
    assert_eq!(state(cx, &rig), (false, true));
    answer(cx, &rig, true);
    assert_eq!(spy.wiped.load(Ordering::SeqCst), 1);
    assert_eq!(state(cx, &rig), (false, false));
}

#[gpui_kit::test]
fn pause_flips_the_record_history_setting(cx: &mut TestAppContext) {
    let (_, hooks) = spy();
    let rig = rig(cx, hooks);
    let records = |cx: &mut TestAppContext| {
        rig.view
            .read_with(cx, |view, _| view.values.records_clipboard())
    };
    assert!(records(cx));
    dispatch(cx, &rig, PauseClipboard);
    assert!(!records(cx));
    dispatch(cx, &rig, PauseClipboard);
    assert!(records(cx));
}

#[test]
fn the_confirms_show_counts_and_name_the_set_aside_copies() {
    use crate::window_view::clipboard_palette::{Ask, ask_props};
    let clear = ask_props(Ask::Clear(COUNTS));
    assert_eq!(clear.lines, ["12 history items"]);
    let wipe = ask_props(Ask::DeleteAll(COUNTS));
    assert_eq!(wipe.lines.len(), 3);
    assert_eq!(wipe.lines[1], "3 saved items and secrets");
    assert!(wipe.summary.contains("clipboard.db.corrupt-*"));
}
