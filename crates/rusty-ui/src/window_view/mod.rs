mod app_menu;
mod chrome_state;
mod clipboard;
mod closing;
mod command_center;
mod crash;
mod drag_hover;
mod drive_menu;
mod drives;
mod drop;
mod drop_external;
mod drop_menu;
mod drop_platform;
mod editor;
mod fkeys;
mod font_settings;
mod hand_off;
mod hints;
mod index_activity;
mod jobs;
mod keys;
mod layout;
mod list_columns;
mod multi_rename;
mod operate;
mod pane;
mod pane_strip;
mod path_edit;
mod perf_recording;
mod placement;
mod preview;
mod prompt;
mod rest;
mod search_hooks;
mod settings_news;
mod settings_save;
mod settings_sheet;
mod shell_menu;
mod spring;
mod start;
mod start_watch;
mod status;
mod strip;
mod strip_keys;
mod tab_keys;
mod terminal;
mod tiles;
mod title;
mod undo;
mod user_keymap;
mod viewer;
mod visits;
mod waiting;
mod watches;
mod window_menu;
mod workspace_keys;
mod workspace_tray;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use command_center::CommandCenterState;
use gpui_kit::{FocusHandle, ModifiersChangedEvent, Subscription};
use rusty_cache::{CacheKeys, ListingCache};
use rusty_core::{AppState, Command, Direction, PaneId};
use rusty_fs::ProviderOps;

use crate::app_menu::{Identity, ToggleAppMenu};
use crate::browser::{Browser, Launcher, Operations, RenameHooks, Waker};
use crate::prelude::*;
use crate::settings::{ToggleSettings, Values};
use crate::title_bar::{GradientLayer, system_backdrop};
use crate::tokens::{self, Mode};

pub use app_menu::AppMenuHooks;
pub use clipboard::{ClipboardEffect, ClipboardFault, ClipboardFiles, FileClipboardService};
pub use crash::CrashReportService;
pub use drives::{DriveHooks, DriveIcon, DriveInfo};
pub use drop_menu::drop_items;
pub use drop_platform::{DropPlatform, invalid_drop_path};
pub use editor::EditorHooks;
pub use fkeys::{
    DELETE_WARNING, FKey, Labels, delete_warning_in, key_labels, key_labels_in, keys_in_layer,
    layer_keys, layer_keys_in, settings_labels_in,
};
pub use hand_off::{OpenHooks, OpenInbox, OpenRequest, OpenSender, open_requests};
pub use keys::bind_keys;
pub(crate) use keys::{bind_app_keys, rebind_app_keys};
pub use placement::DisplayScale;
pub use preview::folder_summary;
pub use search_hooks::{
    CommandUse, Frecent, Frecents, RecentCommands, SearchHooks, ServiceHooks, ServiceStatus,
};
pub use settings_news::{SettingsInbox, SettingsNews, SettingsSender};
pub use settings_save::{SaveReport, SaveSettings};
use settings_sheet::SettingsPanel as Panel;
pub use settings_sheet::{FindLine, KeyLine, SettingsHooks};
pub use shell_menu::{
    BackgroundLook, MenuTarget, Reply, ScreenSpot, ShellHealth, ShellMenuProvider,
    ShellMenuSession, merged_background_menu, merged_file_menu,
};
pub use start::run;
pub use start_watch::{StartReport, StartWatch};
pub use terminal::TerminalHooks;
pub use user_keymap::{KeymapHooks, KeymapNews, KeymapSender};
pub use viewer::{
    ArchiveSource, ArchiveView, Attach, BodyAct, BodyKeys, EDIT_LIMIT, FileHooks, FileStamp,
    FileText, FontFace, FontView, Head, Host, IndexRun, Key, Kind, Kinds, Load, Loaded, MemoryLink,
    Paged, PagedKind, PagedView, Reopen, Save, SaveError, Saved, SharedSource, Slot, Sniff,
    ViewerBody, ViewerKind,
};
pub use viewer::{ExifEntry, IMAGE_LIMIT, ImageView, MenuEntry, Picture, ReadBytes, decode_image};
pub use viewer::{FileBytes, FileBytesBar, FilePart, Legend, RegionHead, UsageBar, UsageLine};
pub use viewer::{MemoryStrip, StripData, StripEvent, StripItem, StripLabel, StripStep};
pub use viewer::{OpenPdf, PdfLook, PdfPages, PdfStart, PdfView};
pub use viewer::{Treemap, TreemapEvent, TreemapGroup, TreemapPart};
pub use viewer::{bind_keys as bind_viewer_keys, encoding_picker, view_menu, viewer_menu};
pub use window_menu::window_view_menu;

pub type SaveSession = Rc<dyn Fn(&AppState)>;

type AfterOpen = (
    KeymapHooks,
    OpenHooks,
    Option<rusty_core::Location>,
    Option<RenameHooks>,
);

pub struct StartOptions {
    pub state: AppState,
    pub provider: Arc<dyn ProviderOps>,
    pub on_save: SaveSession,
    pub mode: Mode,
    pub operations: Operations,
    pub launcher: Launcher,
    pub editor: EditorHooks,
    pub terminal: TerminalHooks,
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
    pub drop_platform: Option<Rc<dyn DropPlatform>>,
    pub start_watch: Option<StartWatch>,
    pub open: OpenHooks,
    pub crash_report: Option<Arc<dyn CrashReportService>>,
    pub search: SearchHooks,
    pub rename: Option<RenameHooks>,
}

pub struct FerriteWindow {
    browser: Browser,
    focus: HashMap<PaneId, FocusHandle>,
    tiles: tiles::TilesState,
    on_save: SaveSession,
    prompt: Option<prompt::Prompt>,
    editor: EditorHooks,
    terminal: TerminalHooks,
    files: FileHooks,
    kinds: viewer::Kinds,
    viewer: Option<viewer::Viewer>,
    next_prompt: u64,
    provider: Arc<dyn ProviderOps>,
    drives: drives::Drives,
    drive_menu: Option<drive_menu::MenuState>,
    drag: drop_platform::DragDrop,
    menu_focus: FocusHandle,
    path_edit: Option<path_edit::PathEdit>,
    waiting: waiting::Waiting,
    rest: rest::Rest,
    hints: hints::Hints,
    app_menu: Option<app_menu::OpenMenu>,
    app_menu_focus: FocusHandle,
    values: Values,
    window_menu: window_menu::WindowMenu,
    identity: Identity,
    saver: settings_save::Saver,
    settings: settings_sheet::SettingsPanel,
    shell_menus: Option<Rc<dyn ShellMenuProvider>>,
    menu_run: Option<shell_menu::MenuRun>,
    prefetch: shell_menu::Prefetches,
    menu_runs: u64,
    preview: preview::Preview,
    job_ui: jobs::JobUi,
    keymap: user_keymap::KeymapState,
    clipboard: clipboard::ClipboardState,
    crash: crash::CrashState,
    command_center: command_center::CommandCenterState,
    multi_rename: multi_rename::MultiRenameState,
    start: Option<start_watch::Watching>,
    chrome: chrome_state::Chromes,
    workspace_keys: workspace_keys::WorkspaceKeys,
    _watches: [Subscription; 5],
}

impl FerriteWindow {
    fn new(options: StartOptions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut browser = Self::browser(&options, cx);
        let totals = options.search.folder_totals.clone();
        let (values, saver, settings) = Panel::split(options.settings, &options.search, cx);
        settings_save::steer(&values, &mut browser);
        let view = Self {
            browser,
            focus: HashMap::new(),
            tiles: tiles::TilesState::default(),
            on_save: options.on_save,
            prompt: None,
            editor: options.editor,
            terminal: options.terminal,
            files: options.files,
            kinds: options.kinds,
            viewer: None,
            next_prompt: 0,
            command_center: CommandCenterState::new(options.search, &options.provider),
            provider: options.provider,
            drives: drives::Drives::new(options.drives),
            drive_menu: None,
            drag: drop_platform::DragDrop::new(options.drop_platform),
            menu_focus: cx.focus_handle(),
            path_edit: None,
            waiting: waiting::Waiting::default(),
            rest: rest::Rest::observe(cx),
            hints: hints::Hints::new(window, cx),
            app_menu: None,
            app_menu_focus: cx.focus_handle(),
            values,
            window_menu: window_menu::WindowMenu::default(),
            identity: options.app_menu.identity,
            saver,
            settings,
            shell_menus: options.shell_menu,
            menu_run: None,
            prefetch: shell_menu::Prefetches::default(),
            menu_runs: 0,
            preview: preview::Preview::new(Self::waker(cx), totals, cx),
            job_ui: jobs::JobUi::new(window, cx),
            keymap: user_keymap::KeymapState::default(),
            clipboard: clipboard::ClipboardState::new(options.file_clipboard),
            crash: crash::CrashState::new(options.crash_report, cx),
            multi_rename: multi_rename::MultiRenameState::default(),
            start: options.start_watch.map(start_watch::Watching::built),
            chrome: chrome_state::Chromes::default(),
            workspace_keys: workspace_keys::WorkspaceKeys::new(window, cx),
            _watches: Self::watches(window, cx),
        };
        let after = (options.keymap, options.open, options.view, options.rename);
        view.after_open(after, window, cx)
    }

    fn after_open(
        mut self,
        (keymap, open, start_view, rename): AfterOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        self.browser.start();
        self.browser.set_rename_hooks(rename);
        self.listen_files(keymap, window, cx);
        self.tell_pending_undo(cx);
        self.listen_open(open, window, cx);
        self.apply_theme(window, cx);
        Self::close_saves_and_quits(window, cx);
        Self::poll_drives(cx);
        Self::poll_index_activity(cx);
        self.check_index_upgrade(cx);
        cx.defer_in(window, move |view, window, cx| {
            view.focus_core_pane(window, cx);
            view.watch_listings(cx);
            if let Some(location) = start_view {
                view.view_location(location, window, cx);
            }
        });
        self
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
        self.note_seen(cx);
        self.clear_finished_cuts();
        self.jobs_woke(cx);
        if polled {
            self.drag_list_changed(cx);
        }
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

    fn apply(&mut self, command: &Command, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.browser.command(command);
        self.drag_list_changed(cx);
        self.forget_clipboard_notice();
        self.show_refusal(result);
        self.focus_core_pane(window, cx);
        cx.notify();
    }
}

impl FerriteWindow {
    fn below_title_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = tokens::current(cx);
        let mica = theme.surfaces.mica();
        let has_mica =
            !theme.is_contrast && system_backdrop().gradient == GradientLayer::BelowTitleBar;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .when(has_mica, |below| below.bg(mica))
            .children(self.crash_notice_view(window, cx))
            .children(self.index_upgrade_view(cx))
            .child(self.body_area(window, cx))
            .children(self.fkey_bar(window, cx))
    }
}

impl Render for FerriteWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_workspaces(window);
        let frame = self.perf_frame(window);
        self.watch_start(window, cx);
        let t = tokens::current(cx).clone();
        let gradient = system_backdrop().gradient;
        let below_title_bar = self.below_title_bar(window, cx);
        let root = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .when(!t.is_contrast && gradient == GradientLayer::Root, |root| {
                root.bg(t.surfaces.mica())
            })
            .when(t.is_contrast, |root| root.bg(t.colors.background))
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
            .children(Self::noted_modal(self.modal_view(window, cx)))
            .children(self.app_menu_view(cx))
            .children(self.shell_menu_view())
            .children(self.viewer_menu_view())
            .children(self.view_menus());
        let root = self.drag_layer(Self::perf_on(root, frame, cx), cx);
        let root = Self::open_window_menu_on(Self::multi_rename_on(root, cx), cx);
        let root = self.command_center_on(self.activity_keys(self.crash_keys(root, cx), cx), cx);
        self.preview_keys(root, window, cx)
    }
}

#[cfg(test)]
pub(crate) mod tests;
