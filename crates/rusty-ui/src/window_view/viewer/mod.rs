mod files;
mod keys_bar;
mod kinds;
mod lister;
mod modes;
mod open;
mod paged;
mod save;
mod view;

use gpui_kit::component::input::{EditorState, InputEvent};
use gpui_kit::{FocusHandle, KeyBinding, Subscription, Task};
use rusty_core::Location;

pub use files::{
    FileHooks, FileStamp, FileText, Head, IndexRun, Paged, Reopen, Save, SaveError, SharedSource,
};
pub use kinds::{EDIT_LIMIT, Kind, Kinds, Shape, Slot, Sniff, ViewerKind};
pub use paged::{PagedKind, PagedView};

use super::FerriteWindow;
use crate::prelude::*;
use open::Opening;

const CONTEXT: &str = "Viewer";
const PAGED_CONTEXT: &str = "Paged";
const LEAVE_WARNING: &str = "Unsaved changes. Ctrl+S saves; Esc again discards them.";

gpui_kit::actions!(
    viewer,
    [
        SaveFile,
        CloseViewer,
        EditFile,
        ShowText,
        ShowHex,
        ShowRich,
        ListerText,
        ListerHex,
        ListerRich,
        FindNext,
        FindPrevious,
        NextFile,
        PreviousFile,
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
        KeyBinding::new("ctrl-e", EditFile, context),
        KeyBinding::new("alt-1", ShowText, context),
        KeyBinding::new("alt-3", ShowHex, context),
        KeyBinding::new("alt-5", ShowRich, context),
        KeyBinding::new("1", ListerText, context),
        KeyBinding::new("3", ListerHex, context),
        KeyBinding::new("5", ListerRich, context),
        KeyBinding::new("f3", FindNext, context),
        KeyBinding::new("shift-f3", FindPrevious, context),
        KeyBinding::new("n", NextFile, context),
        KeyBinding::new("p", PreviousFile, context),
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
    paged: Option<Entity<PagedView>>,
    focus: FocusHandle,
    head: Head,
    kinds: Vec<Kind>,
    kind: Option<Kind>,
    opening: Option<Opening>,
    editing: bool,
    can_edit: bool,
    crlf: bool,
    stamp: Option<FileStamp>,
    dirty: bool,
    revision: u64,
    saving: Option<Task<()>>,
    overwrite: bool,
    leaving: bool,
    switching: Option<Slot>,
    message: Option<SharedString>,
    _changes: Option<Subscription>,
}

impl Viewer {
    fn new(location: Location, focus: FocusHandle) -> Self {
        let title: SharedString = location
            .name()
            .map_or_else(|| location.as_str().to_owned(), |name| name.to_text())
            .into();
        Self {
            location,
            title,
            editor: None,
            paged: None,
            focus,
            head: Head::default(),
            kinds: Vec::new(),
            kind: None,
            opening: None,
            editing: false,
            can_edit: false,
            crlf: false,
            stamp: None,
            dirty: false,
            revision: 0,
            saving: None,
            overwrite: false,
            leaving: false,
            switching: None,
            message: None,
            _changes: None,
        }
    }

    fn sniff(&self) -> Sniff<'_> {
        Sniff {
            head: &self.head.bytes,
            name: &self.title,
            len: self.head.len,
        }
    }

    fn kind_in(&self, slot: Slot) -> Option<Kind> {
        self.kinds.iter().find(|kind| kind.slot() == slot).cloned()
    }
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
        let mut viewer = Viewer::new(location, cx.focus_handle());
        viewer.opening = Some(self.open_file(&viewer, window, cx));
        let focus = viewer.focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        self.viewer = Some(viewer);
        cx.notify();
    }

    fn text_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(viewer) = &mut self.viewer {
            viewer.dirty = true;
            viewer.revision += 1;
            viewer.leaving = false;
            viewer.switching = None;
            viewer.overwrite = false;
        }
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
        let mut state = EditorState::new(window, cx)
            .language(language)
            .default_value(text);
        state.set_readonly(true, cx);
        state
    });
    viewer._changes = Some(cx.subscribe(&editor, |view, _, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change) {
            view.text_changed(cx);
        }
    }));
    viewer.editor = Some(editor);
    viewer.can_edit = !file.read_only;
    viewer.crlf = file.crlf;
    viewer.stamp = file.stamp;
    viewer.message = file
        .read_only
        .then(|| "Not valid UTF-8: opened read-only.".into());
}

#[cfg(test)]
mod tests;
