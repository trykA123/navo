mod body;
mod find;
mod lines;
pub(super) mod lint;
mod marking;
mod overlay;
mod preview;
#[cfg(test)]
pub(super) mod run_probe;
mod save;
mod view;

use std::sync::Arc;

use gpui_kit::component::input::{EditorState, InputEvent, RangeDecorationCollection};
use gpui_kit::{Subscription, Task};
use navo_core::encoding::TextEncoding;
use navo_core::line_ends::LineEnds;
use sidewise_kit::languages::line_highlighter::LineHighlighter;

pub(super) use body::{EditorBody, EditorFile};
pub(super) use preview::SplitChoice;

use super::body::{Carried, Host};
use super::save_as::SaveAs;
use super::search::Marking;
use super::search::crlf::CrlfOffsets;
use super::search::memory::{MemoryLink, attach_memory};
use super::search::overview::Overview;
use super::syntax::Highlight;
use super::{FileStamp, FileText};
use crate::prelude::*;
use crate::settings::{Field, Values};
use crate::tokens::Mode;
use find::EditorFind;
use lint::Lint;
use marking::text_source;
use preview::Split;

pub(super) struct EditorView {
    host: Host,
    editor: Entity<EditorState>,
    editing: bool,
    can_edit: bool,
    crlf: bool,
    ends: Option<Arc<LineEnds>>,
    encoding: TextEncoding,
    save_as: SaveAs,
    stamp: Option<FileStamp>,
    persists: bool,
    dirty: bool,
    /// The text differs from the file on disk; a pending conversion alone leaves it false.
    edited: bool,
    revision: u64,
    overwrite: bool,
    saving: Option<Task<()>>,
    note: Option<SharedString>,
    find: EditorFind,
    go_to: Option<lines::GoToBar>,
    marking: Marking,
    line_numbers: bool,
    gutter: Option<bool>,
    /// The window's soft-wrap choice for edit mode; `wrapped` is what the widget does now.
    soft_wrap: bool,
    wrapped: bool,
    decorations: RangeDecorationCollection,
    decorated: Option<(u64, Mode)>,
    overview: Overview,
    recount: Option<Task<()>>,
    split: Option<Split>,
    lint: Lint,
    _changes: Subscription,
}

fn read_only_note(file: &FileText) -> SharedString {
    if file.ends.as_deref().is_some_and(LineEnds::has_stray_cr) {
        return "Opened read-only: a line ends with a lone CR, so its line endings cannot be kept exactly."
            .into();
    }
    format!(
        "Not valid as {}: bad bytes show as \u{FFFD}. Opened read-only.",
        file.encoding.label()
    )
    .into()
}

fn crlf_offsets(file: &FileText, text: &Arc<str>) -> Option<CrlfOffsets> {
    match &file.ends {
        Some(ends) => Some(CrlfOffsets(Arc::from(ends.crlf_marks(&file.text)))),
        None => file.crlf.then(|| CrlfOffsets(text.clone())),
    }
}

fn memory_link(file: &FileText, text: &Arc<str>, host: &Host) -> (bool, MemoryLink) {
    let persists =
        file.encoding.encoding.name() == "UTF-8" && !file.encoding.bom && !file.read_only;
    let carried = host.carried.clone().map(|carried| Carried {
        bookmarks: carried.bookmarks.filter(|_| persists),
        ..carried
    });
    let link = MemoryLink {
        hooks: host.hooks.clone(),
        location: host.location.clone(),
        stamp: file.stamp.filter(|_| persists),
        carried,
        crlf: crlf_offsets(file, text),
    };
    (persists, link)
}

/// Builds the read-only editor state for `text`, coloured by `highlight` (grammar, line lexer or plain).
fn read_only_state(
    text: String,
    highlight: &Highlight,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> EditorState {
    crate::languages::register_languages();
    let language = match highlight {
        Highlight::Grammar(name) => (*name).to_owned(),
        Highlight::Line(lexer) => lexer.name().to_owned(),
        Highlight::Plain => "text".to_owned(),
    };
    let mut state = EditorState::new(window, cx)
        .language(language)
        .default_value(text);
    if let Highlight::Line(lexer) = highlight {
        let factory = LineHighlighter::factory(lexer.clone());
        #[cfg(test)]
        let factory = run_probe::recording(factory);
        state.set_highlighter_factory(factory, cx);
    }
    state.set_readonly(true, cx);
    state.set_searchable(false, cx);
    state.set_context_menu_enabled(false);
    state
}

/// Calls `text_changed` on every edit of the editor widget; the view keeps the subscription.
fn on_text_change(editor: &Entity<EditorState>, cx: &mut Context<EditorView>) -> Subscription {
    cx.subscribe(editor, |view, _, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change) {
            view.text_changed(cx);
        }
    })
}

impl EditorView {
    fn new(opened: EditorFile, host: Host, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let EditorFile {
            file,
            highlight,
            split,
        } = opened;
        let shared: Arc<str> = Arc::from(file.text.as_str());
        let (persists, link) = memory_link(&file, &shared, &host);
        let note = file.read_only.then(|| read_only_note(&file));
        let lint = Lint::for_location(&host.location);
        let editor = cx.new(|cx| read_only_state(file.text, &highlight, window, cx));
        let decorations = editor.update(cx, |state, cx| {
            state.create_range_decorations_collection(Vec::new(), cx)
        });
        let split = split.map(|choice| Split::new(choice, host.clone(), &editor, cx));
        let changes = on_text_change(&editor, cx);
        let mut view = Self {
            host,
            editor,
            editing: false,
            can_edit: !file.read_only,
            crlf: file.crlf,
            ends: file.ends.clone(),
            encoding: file.encoding,
            save_as: SaveAs::default(),
            stamp: file.stamp,
            persists,
            dirty: false,
            edited: false,
            revision: 0,
            overwrite: false,
            saving: None,
            note,
            find: EditorFind::default(),
            go_to: None,
            marking: Marking::new(text_source(shared)),
            line_numbers: true,
            gutter: None,
            soft_wrap: true,
            wrapped: true,
            decorations,
            decorated: None,
            overview: Overview::default(),
            recount: None,
            split,
            lint,
            _changes: changes,
        };
        attach_memory(&mut view, link, cx);
        view.lint_later(std::time::Duration::ZERO, cx);
        view
    }

    fn show_parts(&mut self, values: &Values, cx: &mut Context<Self>) {
        self.line_numbers = values.flag(Field::TextLineNumbers);
        self.marking.line_ends = values.flag(Field::TextLineEnds);
        self.show_split(values.flag(Field::MarkdownPreviewSplit), cx);
        cx.notify();
    }

    fn sync_line_numbers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self.line_numbers;
        if self.gutter != Some(wanted) {
            self.gutter = Some(wanted);
            self.editor
                .update(cx, |state, cx| state.set_line_number(wanted, window, cx));
        }
    }

    #[cfg(test)]
    pub(super) fn shows_line_numbers(&self) -> bool {
        self.gutter == Some(true)
    }

    /// Applies soft wrap to the widget: the window's choice while editing, always on while viewing.
    fn sync_soft_wrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = !self.editing || self.soft_wrap;
        if self.wrapped != wanted {
            self.wrapped = wanted;
            self.editor
                .update(cx, |state, cx| state.set_soft_wrap(wanted, window, cx));
        }
    }

    #[cfg(test)]
    pub(super) fn wraps(&self) -> bool {
        self.wrapped
    }

    #[cfg(test)]
    pub(super) fn marking_state(&self) -> &Marking {
        &self.marking
    }

    #[cfg(test)]
    pub(super) fn selected(&self, cx: &App) -> std::ops::Range<usize> {
        self.editor.read(cx).selected_range()
    }

    #[cfg(test)]
    pub(super) fn decorated_ranges(&self, cx: &App) -> Vec<std::ops::Range<usize>> {
        self.decorations.get_ranges(cx)
    }

    fn text_changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.edited = true;
        self.revision += 1;
        self.marking.freeze_bookmarks();
        self.overwrite = false;
        self.note = None;
        self.text_edited(cx);
        self.preview_edited(cx);
        self.lint_edited(cx);
        cx.notify();
    }

    fn start_editing(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.can_edit {
            return false;
        }
        if !self.editing {
            self.editing = true;
            self.marking.find_all = None;
            self.editor
                .update(cx, |state, cx| state.set_readonly(false, cx));
            self.refresh_preview(cx);
            cx.notify();
        }
        true
    }
}
