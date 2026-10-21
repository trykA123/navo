use rusty_l10n::tr;

use crate::commands::{GLOBAL, is_within};

pub(super) fn context_name(context: &str) -> Option<String> {
    key_of(context).map(tr)
}

fn key_of(context: &str) -> Option<&'static str> {
    Some(match context {
        GLOBAL => "command-center-context-app",
        "FerritePane" | "PaneWaiting" => "command-center-context-pane",
        "FileList" => "command-center-context-file-list",
        "PreviewBrowse" => "command-center-context-preview",
        "PathInput" => "command-center-context-path-bar",
        "DriveMenu" => "command-center-context-drive-menu",
        "ActivityStrip" | "ActivityDrawer" => "command-center-context-activity",
        "ConfirmDialog" | "PromptDialog" => "command-center-context-dialog",
        "ContextMenu" => "command-center-context-context-menu",
        "CrashNotice" => "command-center-context-crash-notice",
        "Shortcuts" => "command-center-context-shortcuts",
        _ if is_within(context, "Viewer") => "command-center-context-viewer",
        _ if is_within(context, "MultiRename") => "command-center-context-multi-rename",
        _ if is_within(context, "SettingsSheet") => "command-center-context-settings",
        _ => return None,
    })
}
