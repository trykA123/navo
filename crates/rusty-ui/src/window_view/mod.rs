mod app_menu;
mod cached_views;
mod chrome_state;
mod clipboard;
mod closing;
mod command_center;
mod crash;
mod diagnostics_refresh;
mod dialog_input;
mod drag_hover;
mod drive_menu;
mod drives;
mod drop;
mod drop_external;
mod drop_menu;
mod drop_platform;
mod editor;
mod find_tab;
mod first_keys;
mod fkeys;
mod font_settings;
mod hand_off;
mod help;
mod hints;
mod index_activity;
mod input_trace;
mod jobs;
mod keys;
mod lane;
mod layout;
mod layout_mode;
mod layout_switcher;
mod list_columns;
mod multi_rename;
mod operate;
mod pane;
mod pane_header;
mod pane_strip;
mod pane_swap;
mod path_edit;
#[cfg(feature = "tester")]
mod perf_recording;
#[cfg(not(feature = "tester"))]
#[path = "perf_off.rs"]
mod perf_recording;
mod placement;
mod preset_edit;
mod preview;
mod prompt;
mod recent_sheet;
mod render;
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
mod tab_actions;
mod tab_drag;
mod tab_keys;
mod tab_menu;
mod terminal;
mod tiles;
mod title;
mod undo;
mod undo_retained;
mod user_keymap;
mod viewer;
mod visits;
mod waiting;
mod watches;
mod window_menu;
mod workspace_keys;
mod workspace_tray;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use command_center::CommandCenterState;
use gpui_kit::{FocusHandle, Subscription};
use rusty_cache::{CacheKeys, ListingCache};
use rusty_core::{AppState, Command, Direction, PaneId};
use rusty_fs::ProviderOps;

use crate::app_menu::Identity;
use crate::browser::{Browser, Launcher, Operations, RenameHooks, RetainedHooks, Waker};
use crate::prelude::*;
use crate::settings::Values;
use crate::tokens::Mode;

pub use app_menu::AppMenuHooks;
pub use clipboard::{ClipboardEffect, ClipboardFault, ClipboardFiles, FileClipboardService};
pub use crash::CrashReportService;
pub use drives::{DriveChanges, DriveHooks, DriveIcon, DriveInfo, DriveSignal};
pub use drop_menu::drop_items;
pub use drop_platform::{DropPlatform, invalid_drop_path};
pub use editor::EditorHooks;
pub use fkeys::{
    DELETE_WARNING, FKey, Labels, delete_warning_in, key_labels, key_labels_in, keys_in_layer,
    layer_keys, layer_keys_in, settings_labels_in,
};
pub use hand_off::{
    OpenHooks, OpenInbox, OpenRequest, OpenSender, RawWindowHandle, ResidentHooks, WindowShown,
    open_requests,
};
pub use input_trace::InputTrace;
pub use keys::bind_keys;
pub(crate) use keys::{bind_app_keys, rebind_app_keys};
pub use layout_switcher::{
    LayoutSwitcher, LayoutSwitcherAction, builtin_layout_presets, layout_presets,
};
pub use placement::DisplayScale;
pub use preset_edit::{
    EditLayout, LayoutEdit, MAX_LAYOUT_PRESETS, layout_ask_props, no_layout_edits,
    save_layout_props,
};
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
pub use start::{RunHooks, run};
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
pub use workspace_tray::{OnTrayAction, TrayAction, TrayCard, TrayTile, WorkspaceTray};

pub type SaveSession = Rc<dyn Fn(&AppState)>;
pub type BackgroundSaveSession = Arc<dyn Fn(&AppState) + Send + Sync>;

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
    pub file_clipboard: Option<Arc<dyn FileClipboardService>>,
    pub drop_platform: Option<Rc<dyn DropPlatform>>,
    pub start_watch: Option<StartWatch>,
    pub open: OpenHooks,
    pub crash_report: Option<Arc<dyn CrashReportService>>,
    pub search: SearchHooks,
    pub rename: Option<RenameHooks>,
    pub retained: Arc<dyn RetainedHooks>,
}

pub struct FerriteWindow {
    browser: Browser,
    focus: HashMap<PaneId, FocusHandle>,
    tiles: tiles::TilesState,
    pane_swap: pane_swap::PaneSwap,
    tab_drag: tab_drag::TabDrag,
    on_save: SaveSession,
    window_shown: Option<WindowShown>,
    resident: Option<Rc<dyn ResidentHooks>>,
    autosave: closing::Autosave,
    prompt: Option<prompt::Prompt>,
    dialog_input: dialog_input::DialogInput,
    first_keys: first_keys::FirstKeys,
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
    tab_menu: tab_menu::TabMenu,
    identity: Identity,
    saver: settings_save::Saver,
    settings: settings_sheet::SettingsPanel,
    recent: recent_sheet::RecentPanel,
    shell_menus: Option<Rc<dyn ShellMenuProvider>>,
    menu_run: Option<shell_menu::MenuRun>,
    prefetch: shell_menu::Prefetches,
    menu_runs: u64,
    preview: preview::Preview,
    job_ui: jobs::JobUi,
    keymap: user_keymap::KeymapState,
    clipboard: clipboard::ClipboardState,
    lane: lane::Lane,
    crash: crash::CrashState,
    command_center: command_center::CommandCenterState,
    multi_rename: multi_rename::MultiRenameState,
    start: Option<start_watch::Watching>,
    chrome: chrome_state::Chromes,
    cached: cached_views::CachedViews,
    totals: RefCell<status::TotalsCache>,
    workspace_keys: workspace_keys::WorkspaceKeys,
    _watches: [Subscription; 7],
}

impl FerriteWindow {
    #[expect(clippy::too_many_lines, reason = "one field per window part")]
    fn new(options: StartOptions, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let browser = Self::browser(&options, cx);
        let totals = options.search.folder_totals.clone();
        let (values, saver, settings) = Panel::split(options.settings, &options.search, cx);
        let view = Self {
            browser,
            focus: HashMap::new(),
            tiles: tiles::TilesState::default(),
            pane_swap: pane_swap::PaneSwap::default(),
            tab_drag: tab_drag::TabDrag::default(),
            on_save: options.on_save,
            window_shown: None,
            resident: None,
            autosave: closing::Autosave::observe(cx),
            prompt: None,
            dialog_input: dialog_input::DialogInput::default(),
            first_keys: first_keys::FirstKeys::new(window),
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
            tab_menu: tab_menu::TabMenu::default(),
            identity: options.app_menu.identity,
            saver,
            settings,
            recent: recent_sheet::RecentPanel::new(cx),
            shell_menus: options.shell_menu,
            menu_run: None,
            prefetch: shell_menu::Prefetches::default(),
            menu_runs: 0,
            preview: preview::Preview::new(Self::waker(cx), totals, cx),
            job_ui: jobs::JobUi::new(window, cx),
            keymap: user_keymap::KeymapState::default(),
            clipboard: clipboard::ClipboardState::new(options.file_clipboard),
            lane: lane::Lane::new(window, cx),
            crash: crash::CrashState::new(options.crash_report, cx),
            multi_rename: multi_rename::MultiRenameState::default(),
            start: options.start_watch.map(start_watch::Watching::built),
            chrome: chrome_state::Chromes::default(),
            cached: cached_views::CachedViews::new(cx),
            totals: RefCell::default(),
            workspace_keys: workspace_keys::WorkspaceKeys::new(window, cx),
            _watches: Self::watches(window, cx),
        };
        let after = (options.keymap, options.open, options.view, options.rename);
        view.after_open(after, window, cx)
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
        self.release_first_keys(cx);
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
        if matches!(command, Command::PaneCloseTab { .. }) && self.has_only_tab() {
            return self.replace_only_tab(window, cx);
        }
        let result = self.browser.command(command);
        self.drag_list_changed(cx);
        if !matches!(command, Command::PaneScroll { .. }) {
            self.forget_clipboard_notice();
        }
        self.show_refusal(result);
        self.focus_core_pane(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
pub(crate) mod tests;
