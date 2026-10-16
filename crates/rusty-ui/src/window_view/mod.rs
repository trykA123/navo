mod drive_menu;
mod drives;
mod editor;
mod fkeys;
mod keys;
mod operate;
mod pane;
mod path_edit;
mod prompt;
mod shell_menu;
mod status;
mod viewer;
mod waiting;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{FocusHandle, Modifiers, ModifiersChangedEvent, Subscription, WindowAppearance};
use rusty_cache::{CacheKeys, ListingCache};
use rusty_core::{AppState, Command, Direction, PaneId};
use rusty_fs::ProviderOps;

use crate::browser::{Browser, Launcher, Operations, Waker};
use crate::fkey_bar::{FKeyBar, KeyLayer};
use crate::prelude::*;
use crate::primitives::{Splitter, SplitterState};
use crate::title_bar::{FerriteTitleBar, HeaderSearch};
use crate::tokens::{self, Mode};

const JOB_STOP_WAIT: Duration = Duration::from_secs(10);

pub use drives::{DriveHooks, DriveIcon, DriveInfo};
pub use editor::EditorHooks;
pub use fkeys::{FKey, KEY_WARNING, key_labels, layer_keys};
pub use keys::bind_keys;
pub use shell_menu::{Reply, ScreenSpot, ShellMenuProvider, ShellMenuSession};
pub use viewer::{
    ArchiveSource, ArchiveView, Attach, BodyAct, BodyKeys, EDIT_LIMIT, FileHooks, FileStamp,
    FileText, FontFace, FontView, Head, Host, IndexRun, Key, Kind, Kinds, Load, Loaded, MemoryLink,
    Paged, PagedKind, PagedView, Reopen, Save, SaveError, SharedSource, Slot, Sniff, ViewerBody,
    ViewerKind,
};
pub use viewer::{ExifEntry, IMAGE_LIMIT, ImageView, Picture, ReadBytes, decode_image};

pub type SaveSession = Rc<dyn Fn(&AppState)>;

pub struct StartOptions {
    pub state: AppState,
    pub provider: Arc<dyn ProviderOps>,
    pub on_save: SaveSession,
    pub mode: Mode,
    pub operations: Operations,
    pub launcher: Launcher,
    pub editor: EditorHooks,
    pub files: FileHooks,
    pub kinds: Kinds,
    pub drives: DriveHooks,
    pub view: Option<rusty_core::Location>,
    pub listing_cache: ListingCache,
    pub cache_keys: CacheKeys,
    pub shell_menu: Option<Rc<dyn ShellMenuProvider>>,
}

pub fn run(options: StartOptions) {
    crate::application().run(move |cx| {
        let _ = crate::init(options.mode, cx);
        crate::file_list::bind_keys(cx);
        crate::confirm_dialog::bind_keys(cx);
        crate::prompt_dialog::bind_keys(cx);
        crate::context_menu::bind_keys(cx);
        viewer::bind_keys(cx);
        bind_keys(cx);
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
    prompt: Option<prompt::Prompt>,
    editor: EditorHooks,
    files: FileHooks,
    kinds: viewer::Kinds,
    viewer: Option<viewer::Viewer>,
    next_prompt: u64,
    provider: Arc<dyn ProviderOps>,
    drives: drives::Drives,
    drive_menu: Option<drive_menu::MenuState>,
    menu_focus: FocusHandle,
    path_edit: Option<path_edit::PathEdit>,
    waiting: waiting::Waiting,
    held_layer: Option<KeyLayer>,
    shell_menus: Option<Rc<dyn ShellMenuProvider>>,
    menu_run: Option<shell_menu::MenuRun>,
    prefetch: Option<shell_menu::Prefetch>,
    menu_runs: u64,
    _activation: Subscription,
    _appearance: Subscription,
    _watch: Subscription,
}

fn mode_of(appearance: WindowAppearance) -> Mode {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Mode::Dark,
        WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
    }
}

impl FerriteWindow {
    fn new(options: StartOptions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let provider = options.provider.clone();
        let mut browser = Browser::new(options.state, options.provider, Self::waker(cx));
        browser.set_operations(options.operations);
        browser.set_launcher(options.launcher);
        browser.set_listing_cache(options.listing_cache, options.cache_keys);
        browser.start();
        let view = Self {
            browser,
            focus: HashMap::new(),
            split: cx.new(|_| SplitterState::new(0.5)),
            on_save: options.on_save,
            prompt: None,
            editor: options.editor,
            files: options.files,
            kinds: options.kinds,
            viewer: None,
            next_prompt: 0,
            provider,
            drives: drives::Drives::new(options.drives),
            drive_menu: None,
            menu_focus: cx.focus_handle(),
            path_edit: None,
            waiting: waiting::Waiting::default(),
            held_layer: None,
            shell_menus: options.shell_menu,
            menu_run: None,
            prefetch: None,
            menu_runs: 0,
            _activation: cx.observe_window_activation(window, |view, window, cx| {
                view.track_modifiers(window.modifiers(), window, cx);
            }),
            _watch: cx.observe_self(Self::watch_listings),
            _appearance: cx.observe_window_appearance(window, |_, window, cx| {
                tokens::apply(mode_of(window.appearance()), cx);
                cx.notify();
            }),
        };
        tokens::apply(mode_of(window.appearance()), cx);
        Self::close_saves_and_quits(window, cx);
        Self::poll_drives(cx);
        let start_view = options.view;
        cx.defer_in(window, move |view, window, cx| {
            view.focus_core_pane(window, cx);
            view.watch_listings(cx);
            if let Some(location) = start_view {
                view.view_location(location, window, cx);
            }
        });
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
        self.linger_ended(cx);
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

    fn track_modifiers(&mut self, modifiers: Modifiers, window: &Window, cx: &mut Context<Self>) {
        let held = KeyLayer::held(&modifiers).filter(|_| window.is_window_active());
        if held != self.held_layer {
            self.held_layer = held;
            cx.notify();
        }
    }

    fn fkey_bar(&self, cx: &App) -> FKeyBar {
        if let Some(viewer) = &self.viewer {
            return FKeyBar::new("fkeys", viewer.keys(cx));
        }
        let held = self.held_layer;
        let (layer, index, spoken) = KEY_WARNING;
        FKeyBar::new("fkeys", layer_keys(held))
            .layers(held, key_labels())
            .warn(layer, index, spoken)
            .reserve_pill()
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

    fn close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.warn_unsaved(cx) {
            return false;
        }
        let _ = self.browser.shutdown(JOB_STOP_WAIT);
        self.save();
        true
    }

    fn title_bar(cx: &mut Context<Self>) -> FerriteTitleBar {
        let this = cx.weak_entity();
        FerriteTitleBar::new("Ferrite")
            .on_mica()
            .header(HeaderSearch::new())
            .on_close(move |_, cx| {
                if this.update(cx, |view, cx| view.close(cx)).unwrap_or(true) {
                    cx.quit();
                }
            })
    }

    fn close_saves_and_quits(window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            let closing = this.update(cx, |view, cx| view.close(cx)).unwrap_or(true);
            if closing {
                cx.quit();
            }
            closing
        });
    }
}

impl Render for FerriteWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let panes = self.panes();
        let mut views: Vec<AnyElement> = panes
            .iter()
            .map(|pane| {
                let card = self.pane_view(*pane, window, cx);
                div()
                    .size_full()
                    .p(t.spacing.xs)
                    .child(card)
                    .into_any_element()
            })
            .collect();
        let body = match (views.pop(), views.pop()) {
            (Some(second), Some(first)) => Splitter::new("panes", &self.split, first, second)
                .line_on_hover()
                .into_any_element(),
            (Some(only), None) => only,
            _ => div().into_any_element(),
        };
        let body = self.viewer_view(cx).unwrap_or(body);
        let jobs = self.job_strip(cx);
        let dialog = self.dialog(cx).or_else(|| self.prompt_view(cx));
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(t.surfaces.mica())
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .line_height(t.typography.line_height)
            .child(Self::title_bar(cx))
            .child(div().flex().flex_1().min_h_0().px(t.spacing.xs).child(body))
            .children(jobs)
            .on_modifiers_changed(
                cx.listener(|view, event: &ModifiersChangedEvent, window, cx| {
                    view.track_modifiers(event.modifiers, window, cx);
                }),
            )
            .child(self.fkey_bar(cx))
            .children(dialog)
            .children(self.shell_menu_view())
    }
}

#[cfg(test)]
mod tests;
