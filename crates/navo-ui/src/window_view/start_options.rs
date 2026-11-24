//! What a new main window needs from its host at start-up.

use std::rc::Rc;
use std::sync::Arc;

use navo_cache::{CacheKeys, ListingCache};
use navo_core::AppState;
use navo_fs::ProviderOps;

use crate::browser::{Launcher, Operations, RenameHooks, RetainedHooks};
use crate::tokens::Mode;

use super::{
    AppMenuHooks, ClipboardHooks, CrashReportService, DriveHooks, DropPlatform, EditorHooks,
    FileClipboardService, FileHooks, KeymapHooks, Kinds, OpenHooks, SearchHooks, SettingsHooks,
    ShellMenuProvider, StartWatch, TerminalHooks,
};

/// Saves the app state on the UI thread.
pub type SaveSession = Rc<dyn Fn(&AppState)>;
/// Saves the app state off the UI thread.
pub type BackgroundSaveSession = Arc<dyn Fn(&AppState) + Send + Sync>;

/// Everything the main window receives from its host when it starts.
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
    pub view: Option<navo_core::Location>,
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
    pub git: Option<Arc<dyn crate::git::GitHooks>>,
    pub clipboard: Option<Arc<dyn ClipboardHooks>>,
}
