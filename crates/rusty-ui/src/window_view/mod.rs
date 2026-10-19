mod app_menu;
mod clipboard;
mod crash;
mod drive_menu;
mod drives;
mod editor;
mod fkeys;
mod hand_off;
mod hints;
mod jobs;
mod keys;
mod layout;
mod operate;
mod pane;
mod path_edit;
mod placement;
mod preview;
mod prompt;
mod rest;
mod settings_news;
mod settings_save;
mod settings_sheet;
mod shell_menu;
mod start;
mod start_watch;
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
use crate::fkey_bar::KeyLayer;
use crate::prelude::*;
use crate::primitives::SplitterState;
use crate::settings::{ToggleSettings, Values};
use crate::title_bar::{FerriteTitleBar, GradientLayer, HeaderSearch, system_backdrop};
use crate::tokens::{self, Mode};

const JOB_STOP_WAIT: Duration = Duration::from_secs(10);

pub use app_menu::AppMenuHooks;
pub use clipboard::{ClipboardEffect, ClipboardFault, ClipboardFiles, FileClipboardService};
pub use crash::CrashReportService;
pub use drives::{DriveHooks, DriveIcon, DriveInfo};
pub use editor::EditorHooks;
pub use fkeys::{
    DELETE_WARNING, FKey, Labels, delete_warning_in, key_labels, key_labels_in, keys_in_layer,
    layer_keys, layer_keys_in, settings_labels_in,
};
pub use hand_off::{OpenHooks, OpenInbox, OpenRequest, OpenSender, open_requests};
pub use keys::bind_keys;
pub(crate) use keys::{bind_app_keys, rebind_app_keys};
pub use placement::DisplayScale;
pub use settings_news::{SettingsInbox, SettingsNews, SettingsSender};
pub use settings_save::{SaveReport, SaveSettings};
pub use settings_sheet::{FindLine, KeyLine, SettingsHooks};
pub use shell_menu::{Reply, ScreenSpot, ShellMenuProvider, ShellMenuSession};
pub use start::run;
pub use start_watch::{StartReport, StartWatch};
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
    pub file_clipboard: Option<Rc<dyn FileClipboardService>>,
    pub start_watch: Option<StartWatch>,
    pub open: OpenHooks,
    pub crash_report: Option<Arc<dyn CrashReportService>>,
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
    rest: rest::Rest,
    hints: hints::Hints,
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
    clipboard: clipboard::ClipboardState,
    crash: crash::CrashState,
    start: Option<start_watch::Watching>,
    _activation: Subscription,
    _appearance: Subscription,
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
        let (values, saver, settings) = settings_sheet::SettingsPanel::split(options.settings, cx);
        let mut browser = Browser::new(options.state, options.provider.clone(), Self::waker(cx));
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
            provider: options.provider,
            drives: drives::Drives::new(options.drives),
            drive_menu: None,
            menu_focus: cx.focus_handle(),
            path_edit: None,
            waiting: waiting::Waiting::default(),
            rest: rest::Rest::observe(cx),
            hints: hints::Hints::new(window, cx),
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
            clipboard: clipboard::ClipboardState::new(options.file_clipboard),
            crash: crash::CrashState::new(options.crash_report, cx),
            start: options.start_watch.map(start_watch::Watching::built),
            _follow: cx.observe_in(&cx.entity(), window, Self::follow_cursor),
            _activation: cx.observe_window_activation(window, Self::activation_changed),
            _appearance: cx.observe_window_appearance(window, Self::appearance_changed),
        };
        view.listen_files(options.keymap, window, cx);
        view.listen_open(options.open, window, cx);
        view.after_open(options.view, window, cx);
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
        &mut self,
        start_view: Option<rusty_core::Location>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_theme(window, cx);
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
        self.clear_finished_cuts();
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
        if held != self.hints.held {
            self.hints.held = held;
            cx.notify();
        }
        self.hint_modifiers(modifiers, window, cx);
    }

    fn apply(&mut self, command: &Command, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.browser.command(command);
        self.forget_clipboard_notice();
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

    fn close(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if self.warn_unsaved(cx) {
            return false;
        }
        let _ = self.browser.shutdown(JOB_STOP_WAIT);
        self.remember_placement(window, cx);
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
            .on_close(move |window, cx| {
                if this
                    .update(cx, |view, cx| view.close(window, cx))
                    .unwrap_or(true)
                {
                    cx.quit();
                }
            })
    }

    fn close_saves_and_quits(window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let closing = this
                .update(cx, |view, cx| view.close(window, cx))
                .unwrap_or(true);
            if closing {
                cx.quit();
            }
            closing
        });
    }
}

impl Render for FerriteWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.watch_start(window, cx);
        let t = tokens::current(cx).clone();
        let gradient = system_backdrop().gradient;
        let below_title_bar = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .when(gradient == GradientLayer::BelowTitleBar, |below| {
                below.bg(t.surfaces.mica())
            })
            .children(self.crash_notice_view(window, cx))
            .child(self.body_area(window, cx))
            .child(self.fkey_bar(window, cx));
        let dialog = self.dialog(cx).or_else(|| self.prompt_view(cx));
        let menu = self.app_menu_view(cx);
        let root = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .when(gradient == GradientLayer::Root, |root| {
                root.bg(t.surfaces.mica())
            })
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
        let root = self.crash_keys(root, cx);
        self.preview_keys(root, window, cx)
    }
}

#[cfg(test)]
mod tests;
