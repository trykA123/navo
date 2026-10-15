mod files;
mod modes;
mod paged;
mod view;

use gpui_kit::component::input::{EditorState, InputEvent};
use gpui_kit::{FocusHandle, KeyBinding, Subscription};
use rusty_core::{Command, Location};

pub use files::{
    FileHooks, FileStamp, FileText, IndexRun, OpenAs, Paged, Reopen, Save, SaveError, SharedSource,
};
pub use paged::{PagedKind, PagedView};

use super::FerriteWindow;
use crate::prelude::*;

const CONTEXT: &str = "Viewer";
const PAGED_CONTEXT: &str = "Paged";
const LEAVE_WARNING: &str = "Unsaved changes. Ctrl+S saves; Esc again discards them.";
const CHANGED_WARNING: &str = "Changed on disk since it was opened. Ctrl+S again overwrites it.";

gpui_kit::actions!(
    viewer,
    [
        SaveFile,
        CloseViewer,
        ShowText,
        ShowHex,
        RowUp,
        RowDown,
        PageUp,
        PageDown,
        GoStart,
        GoEnd,
        ScrollLeft,
        ScrollRight,
        OpenFind,
        OpenGoTo,
        MatchCase,
        Back,
    ]
);

pub fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("ctrl-s", SaveFile, context),
        KeyBinding::new("escape", CloseViewer, context),
        KeyBinding::new("f3", CloseViewer, context),
        KeyBinding::new("alt-1", ShowText, context),
        KeyBinding::new("alt-3", ShowHex, context),
        KeyBinding::new("up", RowUp, context),
        KeyBinding::new("down", RowDown, context),
        KeyBinding::new("pageup", PageUp, context),
        KeyBinding::new("pagedown", PageDown, context),
        KeyBinding::new("ctrl-home", GoStart, context),
        KeyBinding::new("ctrl-end", GoEnd, context),
        KeyBinding::new("left", ScrollLeft, context),
        KeyBinding::new("right", ScrollRight, context),
        KeyBinding::new("ctrl-f", OpenFind, context),
        KeyBinding::new("ctrl-g", OpenGoTo, context),
        KeyBinding::new("alt-c", MatchCase, context),
        KeyBinding::new("escape", Back, Some(PAGED_CONTEXT)),
    ]);
}

pub(super) struct Viewer {
    location: Location,
    title: SharedString,
    editor: Option<Entity<EditorState>>,
    focus: FocusHandle,
    read_only: bool,
    crlf: bool,
    stamp: Option<FileStamp>,
    dirty: bool,
    overwrite: bool,
    leaving: bool,
    to_hex: bool,
    editable: bool,
    message: Option<SharedString>,
    paged: Option<Entity<PagedView>>,
    _changes: Option<Subscription>,
}

const KNOWN: [&str; 26] = [
    "toml", "json", "css", "html", "go", "java", "lua", "php", "proto", "scala", "swift", "zig",
    "c", "cpp", "tsx", "svelte", "astro", "diff", "graphql", "ejs", "erb", "cmake", "rust",
    "python", "bash", "markdown",
];

pub(super) fn language_of(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower == "makefile" || lower == "gnumakefile" {
        return "make";
    }
    let ext = lower
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map_or("", |(_, ext)| ext);
    match ext {
        "h" => "c",
        "hpp" | "hh" | "cc" | "cxx" => "cpp",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "htm" | "xhtml" => "html",
        "sh" | "zsh" => "bash",
        "kts" | "kt" => "kotlin",
        "cs" => "csharp",
        "py" | "pyi" => "python",
        "rs" => "rust",
        "md" | "mdx" => "markdown",
        "yml" | "yaml" => "yaml",
        "rb" => "ruby",
        "ex" | "exs" => "elixir",
        "mk" => "make",
        _ => KNOWN
            .iter()
            .find(|known| **known == ext)
            .copied()
            .unwrap_or("text"),
    }
}

impl FerriteWindow {
    pub(super) fn view_row(&mut self, row: u64, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.browser.state().active_tab();
        let location = tab
            .listing()
            .and_then(|listing| listing.get(row))
            .filter(|entry| !entry.is_folder)
            .and_then(|entry| {
                tab.location()
                    .join(&rusty_core::ProviderPath::from_text(&entry.name))
                    .ok()
            });
        if let Some(location) = location {
            self.view_location(location, window, cx);
        }
    }

    pub(super) fn view_location(
        &mut self,
        location: Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title: SharedString = location
            .name()
            .map_or_else(|| location.as_str().to_owned(), |name| name.to_text())
            .into();
        let mut viewer = Viewer {
            location,
            title,
            editor: None,
            focus: cx.focus_handle(),
            read_only: true,
            crlf: false,
            stamp: None,
            dirty: false,
            overwrite: false,
            leaving: false,
            to_hex: false,
            editable: true,
            message: None,
            paged: None,
            _changes: None,
        };
        if let Err(message) = self.open_first(&mut viewer, window, cx) {
            viewer.message = Some(message.into());
        }
        let focus = viewer.focus_target(cx);
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        self.viewer = Some(viewer);
        cx.notify();
    }

    fn text_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(viewer) = &mut self.viewer {
            viewer.dirty = true;
            viewer.leaving = false;
            viewer.to_hex = false;
            viewer.overwrite = false;
        }
        cx.notify();
    }

    fn save_viewer(&mut self, cx: &mut Context<Self>) {
        let Some(viewer) = &self.viewer else {
            return;
        };
        let Some(editor) = viewer.editor.clone().filter(|_| !viewer.read_only) else {
            return;
        };
        let text = editor.read(cx).value();
        let save = Save {
            text: &text,
            crlf: viewer.crlf,
            expected: if viewer.overwrite { None } else { viewer.stamp },
        };
        let result = (self.files.write)(&viewer.location, &save);
        if let Some(viewer) = &mut self.viewer {
            viewer.overwrite = false;
            match result {
                Ok(stamp) => {
                    viewer.stamp = stamp;
                    viewer.dirty = false;
                    viewer.message = Some("Saved.".into());
                }
                Err(SaveError::Changed) => {
                    viewer.overwrite = true;
                    viewer.message = Some(CHANGED_WARNING.into());
                }
                Err(SaveError::Failed(error)) => viewer.message = Some(error.into()),
            }
        }
        let _ = self.browser.command(&Command::PaneRefresh);
        cx.notify();
    }

    pub(super) fn warn_unsaved(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(viewer) = self.viewer.as_mut().filter(|viewer| viewer.dirty) else {
            return false;
        };
        if viewer.leaving {
            return false;
        }
        viewer.leaving = true;
        viewer.message = Some(LEAVE_WARNING.into());
        cx.notify();
        true
    }

    fn close_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.warn_unsaved(cx) {
            return;
        }
        self.viewer = None;
        self.focus_core_pane(window, cx);
        cx.notify();
    }
}

fn attach_editor(
    viewer: &mut Viewer,
    file: FileText,
    window: &mut Window,
    cx: &mut Context<FerriteWindow>,
) {
    let language = language_of(&viewer.title);
    let text = file.text;
    let editor = cx.new(|cx| {
        EditorState::new(window, cx)
            .language(language)
            .default_value(text)
    });
    viewer._changes = Some(cx.subscribe(&editor, |view, _, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change) {
            view.text_changed(cx);
        }
    }));
    viewer.editor = Some(editor);
    viewer.read_only = file.read_only;
    viewer.crlf = file.crlf;
    viewer.stamp = file.stamp;
    viewer.message = file
        .read_only
        .then(|| "Not valid UTF-8: opened read-only.".into());
}

#[cfg(test)]
mod tests;
