mod body;
mod editor;
mod files;
mod keys_bar;
mod kinds;
mod lister;
mod modes;
mod open;
mod paged;
mod view;

use gpui_kit::{FocusHandle, KeyBinding};
use rusty_core::Location;

pub use body::{Attach, BodyKeys, Host, Key, ViewerBody};
pub use files::{
    FileHooks, FileStamp, FileText, Head, IndexRun, Paged, Reopen, Save, SaveError, SharedSource,
};
pub use kinds::{EDIT_LIMIT, Kind, Kinds, Loaded, Slot, Sniff, ViewerKind};
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
        FindOnEnter,
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
        KeyBinding::new("enter", FindOnEnter, context),
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
    focus: FocusHandle,
    head: Head,
    kinds: Vec<Kind>,
    kind: Option<Kind>,
    body: Option<Box<dyn ViewerBody>>,
    opening: Option<Opening>,
    leaving: Option<u64>,
    switching: Option<(Slot, u64)>,
    message: Option<SharedString>,
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
            focus,
            head: Head::default(),
            kinds: Vec::new(),
            kind: None,
            body: None,
            opening: None,
            leaving: None,
            switching: None,
            message: None,
        }
    }

    fn sniff(&self) -> Sniff<'_> {
        Sniff {
            head: &self.head.bytes,
            name: &self.title,
            len: self.head.len,
        }
    }

    fn editable_kind(&self) -> Option<Kind> {
        let sniff = self.sniff();
        self.kinds
            .iter()
            .find(|kind| kind.editable(&sniff))
            .cloned()
    }

    fn next_in(&self, slot: Slot) -> Option<Kind> {
        let in_slot: Vec<&Kind> = self
            .kinds
            .iter()
            .filter(|kind| kind.slot() == slot)
            .collect();
        let shown = self.kind.as_ref().map(|kind| kind.id());
        let at = in_slot.iter().position(|kind| Some(kind.id()) == shown);
        let next = at.map_or(0, |at| (at + 1) % in_slot.len());
        in_slot.get(next).map(|kind| Kind::clone(kind))
    }

    fn is_editing(&self, cx: &App) -> bool {
        self.body.as_ref().is_some_and(|body| body.is_editing(cx))
    }

    fn unsaved(&self, cx: &App) -> Option<u64> {
        self.body
            .as_ref()
            .filter(|body| body.is_dirty(cx))
            .map(|body| body.revision(cx))
    }

    fn say(&mut self, note: impl Into<SharedString>, cx: &mut App) {
        let note = note.into();
        match &self.body {
            Some(body) => body.set_note(note, cx),
            None => self.message = Some(note),
        }
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

    pub(super) fn warn_unsaved(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(viewer) = self.viewer.as_mut() else {
            return false;
        };
        let Some(revision) = viewer.unsaved(cx) else {
            return false;
        };
        if viewer.leaving == Some(revision) {
            return false;
        }
        viewer.leaving = Some(revision);
        viewer.say(LEAVE_WARNING, cx);
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

#[cfg(test)]
mod tests;
