mod browse;
mod files;
mod folders;
mod follow;
mod gate;
mod toggle;

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, ElementId, Entity, TestAppContext};
use rusty_core::AppState;
use rusty_fs::{FakeFs, ProviderOps};

use super::{FOLLOW_AFTER, Shown};
use crate::window_view::tests::{open_options, options_for, press, settle, two_panes};
use crate::window_view::{FerriteWindow, FileHooks, StartOptions};

const WITHIN: Duration = Duration::from_secs(10);

pub(super) struct Rig {
    pub window: AnyWindowHandle,
    pub view: Entity<FerriteWindow>,
}

pub(super) fn open_on(
    cx: &mut TestAppContext,
    fs: &Arc<FakeFs>,
    provider: Arc<dyn ProviderOps>,
    files: FileHooks,
) -> Rig {
    let state: AppState = two_panes(fs);
    let options = StartOptions {
        provider,
        files,
        ..options_for(fs.clone(), state)
    };
    let (window, view) = open_options(cx, options);
    view.update(cx, |view, _| view.preview.set_every(Duration::ZERO));
    Rig { window, view }
}

pub(super) fn open_fake(cx: &mut TestAppContext, fs: &Arc<FakeFs>) -> Rig {
    open_on(cx, fs, fs.clone(), FileHooks::none())
}

impl Rig {
    pub fn key(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    pub fn open(&self, cx: &mut TestAppContext) {
        self.key(cx, "ctrl-q");
        self.settle_motion(cx);
    }

    pub fn settle_motion(&self, cx: &mut TestAppContext) {
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        self.redraw(cx);
    }

    pub fn redraw(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        cx.run_until_parked();
    }

    pub fn rest(&self, cx: &mut TestAppContext) {
        cx.executor().advance_clock(FOLLOW_AFTER);
        cx.run_until_parked();
        settle(cx, &self.view);
        self.redraw(cx);
    }

    pub fn read<T>(&self, cx: &mut TestAppContext, read: impl FnOnce(&FerriteWindow) -> T) -> T {
        self.view.read_with(cx, |view, _| read(view))
    }

    pub fn wait_until(
        &self,
        cx: &mut TestAppContext,
        done: impl Fn(&FerriteWindow) -> bool,
    ) -> bool {
        let deadline = Instant::now() + WITHIN;
        while Instant::now() < deadline {
            settle(cx, &self.view);
            cx.run_until_parked();
            if self.read(cx, &done) {
                self.redraw(cx);
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        false
    }

    pub fn label(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) -> Option<String> {
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

    pub fn label_any(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) -> Option<String> {
        let id = id.into();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(id)
                .and_then(|found| found.label().map(str::to_owned))
        })
        .expect("window")
    }

    pub fn shows(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find(id).is_some_and(|found| found.visible())
        })
        .expect("window")
    }

    pub fn click(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) {
        let id = id.into();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.click(id, cx);
        })
        .expect("window");
        settle(cx, &self.view);
        self.redraw(cx);
    }

    pub fn list_has_focus(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            let view = view.read(cx);
            let pane = view.browser.state().focused_pane();
            view.focus
                .get(&pane)
                .is_some_and(|handle| handle.is_focused(window))
        })
        .expect("window")
    }

    pub fn shown_name(&self, cx: &mut TestAppContext) -> Option<String> {
        self.read(cx, |view| match view.preview.shown()? {
            Shown::File(file) => Some(file.target.name()),
            Shown::Folder(folder) => Some(folder.target.name()),
            Shown::Empty => None,
        })
    }

    pub fn rest_until(
        &self,
        cx: &mut TestAppContext,
        done: impl Fn(&FerriteWindow) -> bool,
    ) -> bool {
        let deadline = Instant::now() + WITHIN;
        while Instant::now() < deadline {
            self.rest(cx);
            if self.read(cx, &done) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        false
    }

    pub fn loads(&self, cx: &mut TestAppContext) -> u64 {
        self.read(cx, |view| view.preview.loads())
    }

    pub fn active_pane(&self, cx: &mut TestAppContext) -> u64 {
        self.read(cx, |view| view.browser.state().focused_pane().get())
    }
}
