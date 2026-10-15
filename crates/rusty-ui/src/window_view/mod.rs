mod operate;
mod pane;
mod status;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{FocusHandle, Subscription, WindowAppearance};
use rusty_core::{AppState, Command, Direction, PaneId};
use rusty_fs::ProviderOps;

use crate::browser::{Browser, Operations, Waker};
use crate::fkey_bar::FKeyBar;
use crate::prelude::*;
use crate::primitives::{Splitter, SplitterState};
use crate::title_bar::{FerriteTitleBar, HeaderSearch};
use crate::tokens::{self, Mode};

const JOB_STOP_WAIT: Duration = Duration::from_secs(10);

pub type SaveSession = Rc<dyn Fn(&AppState)>;

pub struct StartOptions {
    pub state: AppState,
    pub provider: Arc<dyn ProviderOps>,
    pub on_save: SaveSession,
    pub mode: Mode,
    pub operations: Operations,
}

pub fn run(options: StartOptions) {
    crate::application().run(move |cx| {
        let _ = crate::init(options.mode, cx);
        crate::file_list::bind_keys(cx);
        crate::confirm_dialog::bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("ferrite".into()),
            ..crate::window_options("Ferrite")
        };
        let opened = crate::open_window(window_options, cx, move |window, cx| {
            cx.new(|cx| FerriteWindow::new(options, window, cx))
        });
        if opened.is_err() {
            cx.quit();
        }
    });
}

pub struct FerriteWindow {
    browser: Browser,
    focus: HashMap<PaneId, FocusHandle>,
    split: Entity<SplitterState>,
    on_save: SaveSession,
    _appearance: Subscription,
}

fn mode_of(appearance: WindowAppearance) -> Mode {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Mode::Dark,
        WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
    }
}

impl FerriteWindow {
    fn new(options: StartOptions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut browser = Browser::new(options.state, options.provider, Self::waker(cx));
        browser.set_operations(options.operations);
        browser.start();
        let view = Self {
            browser,
            focus: HashMap::new(),
            split: cx.new(|_| SplitterState::new(0.5)),
            on_save: options.on_save,
            _appearance: cx.observe_window_appearance(window, |_, window, cx| {
                tokens::apply(mode_of(window.appearance()), cx);
                cx.notify();
            }),
        };
        tokens::apply(mode_of(window.appearance()), cx);
        Self::close_saves_and_quits(window, cx);
        cx.defer_in(window, |view, window, cx| view.focus_core_pane(window, cx));
        view
    }

    #[cfg(not(test))]
    fn waker(cx: &mut Context<Self>) -> Waker {
        use futures::StreamExt;
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<()>();
        cx.spawn(
            async move |this, cx| {
                while receiver.next().await.is_some() && this.update(cx, Self::wake).is_ok() {}
            },
        )
        .detach();
        Arc::new(move || {
            let _ = sender.unbounded_send(());
        })
    }

    #[cfg(test)]
    fn waker(_: &mut Context<Self>) -> Waker {
        Arc::new(|| {})
    }

    fn wake(&mut self, cx: &mut Context<Self>) {
        if self.browser.poll() {
            cx.notify();
        }
    }

    fn handle(&mut self, pane: PaneId, cx: &mut Context<Self>) -> FocusHandle {
        self.focus
            .entry(pane)
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    fn focus_core_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.browser.state().focused_pane();
        let handle = self.handle(pane, cx);
        window.focus(&handle, cx);
    }

    fn focus_pane(&mut self, pane: PaneId) {
        for direction in Direction::ALL {
            if self.browser.state().focused_pane() == pane {
                return;
            }
            let _ = self.browser.command(&Command::LayoutFocus { direction });
        }
    }

    fn apply(&mut self, command: &Command, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.browser.command(command);
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    fn panes(&self) -> Vec<PaneId> {
        let state = self.browser.state();
        state
            .window(state.focused_window())
            .map(|w| w.panes().iter().map(|p| p.id()).collect())
            .unwrap_or_default()
    }

    fn switch_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.browser.state().focused_pane();
        if let Some(other) = self.panes().into_iter().find(|p| *p != focused) {
            self.focus_pane(other);
            self.focus_core_pane(window, cx);
            cx.notify();
        }
    }

    fn save(&self) {
        (self.on_save)(self.browser.state());
    }

    fn close(&mut self) {
        let _ = self.browser.shutdown(JOB_STOP_WAIT);
        self.save();
    }

    fn title_bar(cx: &mut Context<Self>) -> FerriteTitleBar {
        let this = cx.weak_entity();
        FerriteTitleBar::new("Ferrite")
            .header(HeaderSearch::new())
            .on_close(move |_, cx| {
                let _ = this.update(cx, |view, _| view.close());
                cx.quit();
            })
    }

    fn close_saves_and_quits(window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = this.update(cx, |view, _| view.close());
            cx.quit();
            true
        });
    }
}

impl Render for FerriteWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let panes = self.panes();
        let mut views: Vec<AnyElement> = panes
            .iter()
            .map(|pane| self.pane_view(*pane, window, cx).into_any_element())
            .collect();
        let body = match (views.pop(), views.pop()) {
            (Some(second), Some(first)) => {
                Splitter::new("panes", &self.split, first, second).into_any_element()
            }
            (Some(only), None) => only,
            _ => div().into_any_element(),
        };
        let jobs = self.job_strip(cx);
        let dialog = self.dialog(cx);
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(t.colors.surface)
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .line_height(t.typography.line_height)
            .child(Self::title_bar(cx))
            .child(div().flex().flex_1().min_h_0().child(body))
            .children(jobs)
            .child(FKeyBar::new("fkeys", status::FKEYS))
            .children(dialog)
    }
}

#[cfg(test)]
mod tests;
