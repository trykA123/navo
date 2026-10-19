use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_core::Location;
use rusty_fs::FakeFs;

use super::{fake, open_options, options_for, settle, two_panes};
use crate::window_view::{CrashReportService, FerriteWindow, StartOptions};

const ZIP: &str = "ferrite-report-20261019-0815.zip";

struct FakeReport {
    saved: Result<Location, String>,
    saves: AtomicUsize,
    dismissals: AtomicUsize,
}

impl CrashReportService for FakeReport {
    fn save(&self) -> Result<Location, String> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        self.saved.clone()
    }

    fn dismiss(&self) {
        self.dismissals.fetch_add(1, Ordering::SeqCst);
    }
}

pub(super) struct Rig {
    pub(super) window: AnyWindowHandle,
    pub(super) view: Entity<FerriteWindow>,
    fs: Arc<FakeFs>,
    report: Option<Arc<FakeReport>>,
}

fn rig_with(cx: &mut TestAppContext, saved: Option<Result<&str, &str>>) -> Rig {
    let fs = fake();
    fs.add_folder("home/Downloads");
    fs.add_file(&format!("home/Downloads/{ZIP}"), 2048);
    let report = saved.map(|saved| {
        Arc::new(FakeReport {
            saved: saved.map(|path| fs.location(path)).map_err(str::to_owned),
            saves: AtomicUsize::new(0),
            dismissals: AtomicUsize::new(0),
        })
    });
    let options = StartOptions {
        crash_report: report
            .clone()
            .map(|report| report as Arc<dyn CrashReportService>),
        ..options_for(fs.clone(), two_panes(&fs))
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    Rig {
        window,
        view,
        fs,
        report,
    }
}

pub(super) fn unclean(cx: &mut TestAppContext) -> Rig {
    rig_with(cx, Some(Ok(&format!("home/Downloads/{ZIP}"))))
}

impl Rig {
    fn report(&self) -> &FakeReport {
        self.report.as_deref().expect("a pending crash")
    }

    pub(super) fn is_shown(&self, cx: &mut TestAppContext) -> bool {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find("crash-notice-card")
                .is_some_and(|found| found.visible())
        })
        .expect("window")
    }

    fn has_error(&self, cx: &mut TestAppContext) -> bool {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find("crash-notice-error").is_some()
        })
        .expect("window")
    }

    fn click(&self, cx: &mut TestAppContext, id: &'static str) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.click(id, cx);
        })
        .expect("window");
        settle(cx, &self.view);
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.press(key, cx);
        })
        .expect("window");
        settle(cx, &self.view);
    }

    fn list_has_focus(&self, cx: &mut TestAppContext) -> bool {
        let handle = self.view.update(cx, |view, cx| {
            let pane = view.browser.state().focused_pane();
            view.handle(pane, cx)
        });
        cx.update_window(self.window, |_, window, _| handle.is_focused(window))
            .expect("window")
    }

    fn active_tab(&self, cx: &mut TestAppContext) -> (usize, Location, Option<String>) {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            let pane = state.pane(state.focused_pane()).expect("focused pane");
            let tab = pane.active();
            let cursor = tab
                .listing()
                .and_then(|listing| listing.get(tab.view().cursor))
                .map(|entry| entry.name.clone());
            (pane.tabs().len(), tab.location().clone(), cursor)
        })
    }

    fn status(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            view.status_notice().map(|notice| notice.text.to_string())
        })
    }
}

#[gpui_kit::test]
fn the_notice_shows_after_an_unclean_start_and_leaves_focus_on_the_list(cx: &mut TestAppContext) {
    let rig = unclean(cx);
    assert!(rig.is_shown(cx));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn no_notice_after_a_clean_start(cx: &mut TestAppContext) {
    let rig = rig_with(cx, None);
    assert!(!rig.is_shown(cx));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn save_report_opens_a_tab_on_the_zip_and_says_where_it_went(cx: &mut TestAppContext) {
    let rig = unclean(cx);
    let (tabs, _, _) = rig.active_tab(cx);
    rig.click(cx, "crash-notice-save");
    assert_eq!(rig.report().saves.load(Ordering::SeqCst), 1);
    assert_eq!(
        rig.active_tab(cx),
        (
            tabs + 1,
            rig.fs.location("home/Downloads"),
            Some(ZIP.to_owned())
        )
    );
    assert_eq!(rig.status(cx), Some(format!("Saved {ZIP} in Downloads")));
    assert!(!rig.is_shown(cx));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn a_failed_save_shows_the_error_and_keeps_the_notice(cx: &mut TestAppContext) {
    let rig = rig_with(cx, Some(Err("The disk is full")));
    let before = rig.active_tab(cx);
    rig.click(cx, "crash-notice-save");
    assert_eq!(rig.report().saves.load(Ordering::SeqCst), 1);
    assert!(rig.is_shown(cx));
    assert!(rig.has_error(cx));
    let error = rig.view.read_with(cx, |view, _| view.crash.error.clone());
    assert_eq!(error.as_deref(), Some("The disk is full"));
    assert_eq!(rig.active_tab(cx), before);
    assert_eq!(rig.report().dismissals.load(Ordering::SeqCst), 0);
}

#[gpui_kit::test]
fn dismiss_clears_the_state_and_hides_the_notice(cx: &mut TestAppContext) {
    let rig = unclean(cx);
    rig.click(cx, "crash-notice-dismiss");
    assert_eq!(rig.report().dismissals.load(Ordering::SeqCst), 1);
    assert_eq!(rig.report().saves.load(Ordering::SeqCst), 0);
    assert!(!rig.is_shown(cx));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn esc_on_the_focused_notice_dismisses_it(cx: &mut TestAppContext) {
    let rig = unclean(cx);
    rig.press(cx, "ctrl-shift-r");
    assert!(!rig.list_has_focus(cx));
    rig.press(cx, "escape");
    assert_eq!(rig.report().dismissals.load(Ordering::SeqCst), 1);
    assert!(!rig.is_shown(cx));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn enter_on_the_focused_notice_saves_the_report(cx: &mut TestAppContext) {
    let rig = unclean(cx);
    rig.press(cx, "ctrl-shift-r");
    rig.press(cx, "enter");
    assert_eq!(rig.report().saves.load(Ordering::SeqCst), 1);
    assert!(!rig.is_shown(cx));
}

#[gpui_kit::test]
fn the_reach_key_does_nothing_without_a_notice(cx: &mut TestAppContext) {
    let rig = rig_with(cx, None);
    rig.press(cx, "ctrl-shift-r");
    assert!(rig.list_has_focus(cx));
}
