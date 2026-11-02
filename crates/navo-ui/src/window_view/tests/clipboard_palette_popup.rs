use std::sync::{Arc, Mutex};
use std::time::Instant;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext};
use navo_core::Location;

use super::{fake, open_options, options_for, press, settle, two_panes};
use crate::clip_palette::window::{ClipPopUp, current};
use crate::clip_palette::{
    ClipCounts, ClipId, ClipKind, ClipMode, ClipResults, ClipRow, ClipboardHooks, How, Origin,
    SavedDraft, Scope, SecretText,
};
use crate::window_view::{
    ClipboardSummon, NavoWindow, OpenRequest, OpenSender, PasteTarget, WindowShown, open_requests,
};

type Log = Arc<Mutex<Vec<&'static str>>>;

fn note(log: &Log, entry: &'static str) {
    log.lock().expect("log").push(entry);
}

/// One history row; `copy` notes "write" once its write is done, as the contract says.
struct Board {
    log: Log,
    copies: Mutex<Vec<How>>,
}

fn refused<T>() -> Result<T, String> {
    Err("refused".to_owned())
}

impl ClipboardHooks for Board {
    fn counts(&self) -> Option<ClipCounts> {
        Some(ClipCounts::default())
    }
    fn modes(&self) -> Vec<ClipMode> {
        vec![ClipMode::All]
    }
    fn search(&self, _: &str, _: ClipMode, _: Option<&Location>) -> ClipResults {
        let row = ClipRow {
            id: ClipId(1),
            kind: ClipKind::Text,
            title: "hello".into(),
            detail: String::new(),
            source_app: None,
            when_ms: 0,
            scope: Scope::Everywhere,
            from: Origin::Clipboard,
            ranges: Vec::new(),
        };
        ClipResults {
            rows: vec![row],
            kept: 1,
            revision: 1,
            notice: None,
        }
    }
    fn copy(&self, _: ClipId, how: How) -> Result<(), String> {
        std::thread::sleep(std::time::Duration::from_millis(20));
        self.copies.lock().expect("copies").push(how);
        note(&self.log, "write");
        Ok(())
    }
    fn delete(&self, _: ClipId) -> Result<(), String> {
        refused()
    }
    fn clear_history(&self) -> Result<(), String> {
        refused()
    }
    fn delete_all(&self) -> Result<(), String> {
        refused()
    }
    fn set_recording(&self, _: bool) -> Result<(), String> {
        refused()
    }
    fn subscribe(&self, _: Box<dyn Fn() + Send + Sync>) {}
    fn save_from_history(&self, _: ClipId) -> Result<ClipId, String> {
        refused()
    }
    fn save(&self, _: SavedDraft) -> Result<ClipId, String> {
        refused()
    }
    fn note_for(&self, _: &Location) -> Option<String> {
        None
    }
    fn set_note(&self, _: Scope, _: String) -> Result<(), String> {
        refused()
    }
    fn reveal(&self, _: ClipId) -> Result<SecretText, String> {
        refused()
    }
    fn copy_user(&self, _: ClipId) -> Result<(), String> {
        refused()
    }
}

struct Target {
    log: Log,
    own: bool,
}

impl PasteTarget for Target {
    fn paste(&self) -> Result<(), String> {
        note(&self.log, "paste");
        Ok(())
    }
    fn is_own(&self) -> bool {
        self.own
    }
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    sender: OpenSender,
    log: Log,
    board: Arc<Board>,
}

fn rig(cx: &mut TestAppContext) -> Rig {
    let log = Log::default();
    let board = Arc::new(Board {
        log: log.clone(),
        copies: Mutex::default(),
    });
    let fs = fake();
    let (sender, inbox) = open_requests();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    options.open.inbox = Some(inbox);
    let shown = log.clone();
    let hook: WindowShown = Arc::new(move |_, is_shown| {
        note(&shown, if is_shown { "show" } else { "hide" });
        true
    });
    options.open.window_shown = Some(hook);
    options.clipboard = Some(board.clone());
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    Rig {
        window,
        view,
        sender,
        log,
        board,
    }
}

impl Rig {
    fn summon(&self, cx: &mut TestAppContext, own: bool) {
        let target = Target {
            log: self.log.clone(),
            own,
        };
        let summon = ClipboardSummon {
            at: Instant::now(),
            previous: Some(Arc::new(target)),
            display: None,
        };
        self.send(
            cx,
            OpenRequest {
                clipboard: Some(summon),
                ..OpenRequest::default()
            },
        );
    }

    fn send(&self, cx: &mut TestAppContext, request: OpenRequest) {
        assert!(self.sender.send(request));
        cx.run_until_parked();
    }

    fn log(&self) -> Vec<&'static str> {
        self.log.lock().expect("log").clone()
    }

    fn pop_up(&self, cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<ClipPopUp>) {
        cx.update(|cx| current(cx)).expect("a pop-up")
    }

    fn pop_up_state(&self, cx: &mut TestAppContext) -> (bool, bool) {
        let (_, pop_up) = self.pop_up(cx);
        pop_up.read_with(cx, |pop_up, _| (pop_up.is_shown(), pop_up.has_target()))
    }

    fn press_in_pop_up(&self, cx: &mut TestAppContext, key: &str) {
        let (window, _) = self.pop_up(cx);
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.press(key, cx);
        })
        .expect("pop-up window");
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn a_clipboard_request_shows_the_pop_up_and_leaves_the_main_window_alone(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.summon(cx, false);
    assert_eq!(rig.pop_up_state(cx), (true, true));
    assert_eq!(cx.windows().len(), 2);
    assert!(rig.log().is_empty(), "the main window was not shown");
}

#[gpui_kit::test]
fn escape_hides_the_pop_up_and_a_second_request_reuses_it(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.summon(cx, false);
    let (first, _) = rig.pop_up(cx);
    rig.press_in_pop_up(cx, "escape");
    assert_eq!(rig.pop_up_state(cx), (false, false));
    rig.summon(cx, false);
    let (second, _) = rig.pop_up(cx);
    assert_eq!(first, second);
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(rig.pop_up_state(cx), (true, true));
    assert_eq!(rig.log(), ["hide", "show"]);
}

#[gpui_kit::test]
fn a_hidden_pop_up_keeps_no_rows_until_it_is_shown_again(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.summon(cx, false);
    let (_, pop_up) = rig.pop_up(cx);
    cx.run_until_parked();
    assert!(pop_up.read_with(cx, |pop_up, cx| pop_up.has_rows(cx)));
    rig.press_in_pop_up(cx, "escape");
    cx.run_until_parked();
    assert!(!pop_up.read_with(cx, |pop_up, cx| pop_up.has_rows(cx)));
    rig.summon(cx, false);
    cx.run_until_parked();
    assert!(pop_up.read_with(cx, |pop_up, cx| pop_up.has_rows(cx)));
}

#[gpui_kit::test]
fn focus_loss_hides_the_pop_up(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.summon(cx, false);
    let (window, _) = rig.pop_up(cx);
    cx.update_window(window, |_, window, _| window.activate_window())
        .expect("pop-up window");
    cx.run_until_parked();
    VisualTestContext::from_window(window, cx).deactivate_window();
    cx.run_until_parked();
    assert_eq!(rig.pop_up_state(cx), (false, false));
    assert_eq!(rig.log(), ["hide"]);
}

#[gpui_kit::test]
fn enter_pastes_into_the_previous_window_after_the_write_has_finished(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.summon(cx, false);
    rig.press_in_pop_up(cx, "enter");
    assert_eq!(rig.log(), ["write", "hide", "paste"]);
    assert_eq!(*rig.board.copies.lock().expect("copies"), [How::Paste]);
    assert_eq!(rig.pop_up_state(cx), (false, false));
}

#[gpui_kit::test]
fn ctrl_c_from_anywhere_only_copies_and_stays_open(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.summon(cx, false);
    rig.press_in_pop_up(cx, "ctrl-c");
    assert_eq!(rig.log(), ["write"]);
    assert_eq!(rig.pop_up_state(cx), (true, true));
}

#[gpui_kit::test]
fn navo_s_own_window_as_the_previous_one_only_gets_the_copy(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.summon(cx, true);
    rig.press_in_pop_up(cx, "enter");
    assert_eq!(rig.log(), ["write", "hide"]);
}

#[gpui_kit::test]
fn a_plain_summon_after_a_clipboard_request_never_pastes(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.summon(cx, false);
    rig.press_in_pop_up(cx, "escape");
    rig.send(
        cx,
        OpenRequest {
            summoned_at: Some(Instant::now()),
            ..OpenRequest::default()
        },
    );
    assert_eq!(rig.pop_up_state(cx), (false, false));
    press(cx, rig.window, &rig.view, "ctrl-shift-v");
    press(cx, rig.window, &rig.view, "enter");
    assert_eq!(rig.log(), ["hide", "show", "write"]);
    assert_eq!(*rig.board.copies.lock().expect("copies"), [How::Copy]);
}
