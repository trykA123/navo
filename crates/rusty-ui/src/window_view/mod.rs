mod app_menu;
mod drive_menu;
mod drives;
mod editor;
mod fkeys;
mod jobs;
mod keys;
mod layout;
mod operate;
mod pane;
mod path_edit;
mod preview;
mod prompt;
mod settings_news;
mod settings_save;
mod settings_sheet;
mod shell_menu;
mod status;
mod tab_keys;
mod user_keymap;
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

use crate::app_menu::{Identity, ToggleAppMenu};
use crate::browser::{Browser, Launcher, Operations, Waker};
use crate::fkey_bar::{FKeyBar, KeyLayer};
use crate::prelude::*;
use crate::primitives::SplitterState;
use crate::settings::{ToggleSettings, Values};
use crate::title_bar::{FerriteTitleBar, GradientLayer, HeaderSearch, system_backdrop};
use crate::tokens::{self, Mode};

const JOB_STOP_WAIT: Duration = Duration::from_secs(10);

pub use app_menu::AppMenuHooks;
pub use drives::{DriveHooks, DriveIcon, DriveInfo};
pub use editor::EditorHooks;
pub use fkeys::{
    DELETE_WARNING, FKey, Labels, delete_warning_in, key_labels, key_labels_in, keys_in_layer,
    layer_keys, layer_keys_in, settings_labels_in,
};
pub use keys::bind_keys;
pub(crate) use keys::{bind_app_keys, rebind_app_keys};
pub use settings_news::{SettingsInbox, SettingsNews, SettingsSender};
pub use settings_save::{SaveReport, SaveSettings};
pub use settings_sheet::{FindLine, SettingsHooks};
pub use shell_menu::{Reply, ScreenSpot, ShellMenuProvider, ShellMenuSession};
pub use user_keymap::{KeymapHooks, KeymapNews, KeymapSender};
pub use viewer::{
    ArchiveSource, ArchiveView, Attach, BodyAct, BodyKeys, EDIT_LIMIT, FileHooks, FileStamp,
    FileText, FontFace, FontView, Head, Host, IndexRun, Key, Kind, Kinds, Load, Loaded, MemoryLink,
    Paged, PagedKind, PagedView, Reopen, Save, SaveError, SharedSource, Slot, Sniff, ViewerBody,
    ViewerKind,
};
pub use viewer::{ExifEntry, IMAGE_LIMIT, ImageView, MenuEntry, Picture, ReadBytes, decode_image};
pub use viewer::{bind_keys as bind_viewer_keys, viewer_menu};

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
    pub app_menu: AppMenuHooks,
    pub settings: SettingsHooks,
    pub keymap: KeymapHooks,
    pub view: Option<rusty_core::Location>,
    pub listing_cache: ListingCache,
    pub cache_keys: CacheKeys,
    pub shell_menu: Option<Rc<dyn ShellMenuProvider>>,
}

pub fn run(options: StartOptions) {
    crate::application().run(move |cx| {
        let _ = crate::init(options.mode, cx);
        bind_app_keys(cx);
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("ferrite".into()),
            ..crate::title_bar::main_window_options("Ferrite")
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
    app_menu: Option<app_menu::OpenMenu>,
    app_menu_focus: FocusHandle,
    values: Values,
    identity: Identity,
    saver: settings_save::Saver,
    settings: settings_sheet::SettingsPanel,
    shell_menus: Option<Rc<dyn ShellMenuProvider>>,
    menu_run: Option<shell_menu::MenuRun>,
    prefetch: Option<shell_menu::Prefetch>,
    menu_runs: u64,
    preview: preview::Preview,
    job_ui: jobs::JobUi,
    keymap: user_keymap::KeymapState,
    _activation: Subscription,
    _appearance: Subscription,
    _watch: Subscription,
    _follow: Subscription,
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
        let (values, saver, settings) = settings_sheet::SettingsPanel::split(options.settings, cx);
        let mut browser = Browser::new(options.state, options.provider, Self::waker(cx));
        browser.set_operations(options.operations);
        browser.set_launcher(options.launcher);
        browser.set_listing_cache(options.listing_cache, options.cache_keys);
        settings_save::steer_browser(&values, &mut browser);
        browser.start();
        let mut view = Self {
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
            app_menu: None,
            app_menu_focus: cx.focus_handle(),
            values,
            identity: options.app_menu.identity,
            saver,
            settings,
            shell_menus: options.shell_menu,
            menu_run: None,
            prefetch: None,
            menu_runs: 0,
            preview: preview::Preview::new(Self::waker(cx), cx),
            job_ui: jobs::JobUi::default(),
            keymap: user_keymap::KeymapState::default(),
            _follow: cx.observe_in(&cx.entity(), window, |view, _, window, cx| {
                view.follow_cursor(window, cx);
            }),
            _activation: cx.observe_window_activation(window, Self::activation_changed),
            _watch: cx.observe_self(Self::watch_listings),
            _appearance: cx.observe_window_appearance(window, Self::appearance_changed),
        };
        view.apply_theme(window, cx);
        view.listen_files(options.keymap, window, cx);
        Self::after_open(options.view, window, cx);
        view
    }

    fn appearance_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_theme(window, cx);
        cx.notify();
    }

    fn activation_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.track_modifiers(window.modifiers(), window, cx);
        if window.is_window_active() && self.browser.revalidate_unwatched() {
            cx.notify();
        }
    }

    fn after_open(
        start_view: Option<rusty_core::Location>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Self::close_saves_and_quits(window, cx);
        Self::poll_drives(cx);
        cx.defer_in(window, move |view, window, cx| {
            view.focus_core_pane(window, cx);
            view.watch_listings(cx);
            if let Some(location) = start_view {
                view.view_location(location, window, cx);
            }
        });
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
        let polled = self.browser.poll();
        self.jobs_woke(cx);
        if self.poll_preview() || polled {
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

    fn fkey_bar(&self, window: &Window, cx: &mut Context<Self>) -> FKeyBar {
        let held = self.held_layer;
        let bar = match &self.viewer {
            Some(viewer) => FKeyBar::new("fkeys", viewer.keys(cx)),
            None if self.settings.is_open() => {
                let labels = settings_labels_in(crate::commands::active(cx));
                FKeyBar::new("fkeys", keys_in_layer(&labels, held)).layers(held, labels)
            }
            None => {
                let keymap = crate::commands::active(cx);
                let bar = FKeyBar::new("fkeys", layer_keys_in(keymap, held))
                    .layers(held, key_labels_in(keymap));
                match delete_warning_in(keymap) {
                    Some((layer, index)) => bar.warn(layer, index, DELETE_WARNING),
                    None => bar,
                }
            }
        };
        self.with_job_pill(bar, window, cx)
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

    fn title_bar(&self, cx: &mut Context<Self>) -> FerriteTitleBar {
        let this = cx.weak_entity();
        let toggle = cx.listener(|view: &mut Self, _: &ClickEvent, window, cx| {
            view.toggle_app_menu(window, cx);
        });
        FerriteTitleBar::new("Ferrite")
            .on_mica()
            .app_button(self.app_menu.is_some(), toggle)
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
        let gradient = system_backdrop().gradient;
        let below_title_bar = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .when(gradient == GradientLayer::BelowTitleBar, |below| below.bg(t.surfaces.mica()))
            .child(self.body_area(window, cx))
            .child(self.fkey_bar(window, cx));
        let dialog = self.dialog(cx).or_else(|| self.prompt_view(cx));
        let menu = self.app_menu_view(cx);
        let root = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .when(gradient == GradientLayer::Root, |root| root.bg(t.surfaces.mica()))
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .line_height(t.typography.line_height)
            .child(self.title_bar(cx))
            .child(below_title_bar)
            .on_modifiers_changed(
                cx.listener(|view, event: &ModifiersChangedEvent, window, cx| {
                    view.track_modifiers(event.modifiers, window, cx);
                }),
            )
            .on_action(cx.listener(|view, _: &ToggleAppMenu, window, cx| {
                view.toggle_app_menu(window, cx);
            }))
            .on_action(cx.listener(|view, _: &ToggleSettings, window, cx| {
                view.toggle_settings(window, cx);
            }))
            .on_action(cx.listener(|view, _: &keys::ToggleJobs, _, cx| view.jobs_key(cx)))
            .children(dialog)
            .children(menu)
            .children(self.shell_menu_view())
            .children(self.viewer_menu_view());
        self.preview_keys(root, window, cx)
    }
}

#[cfg(test)]
mod tests;
