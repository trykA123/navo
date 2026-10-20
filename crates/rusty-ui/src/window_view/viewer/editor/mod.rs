mod body;
mod find;
mod lines;
mod marking;
mod overlay;
mod preview;
mod save;
mod view;

use std::sync::Arc;

use gpui_kit::component::input::{EditorState, InputEvent, RangeDecorationCollection};
use gpui_kit::{Subscription, Task};
use rusty_core::encoding::TextEncoding;
use rusty_core::line_ends::LineEnds;

pub(super) use body::{EditorBody, EditorFile};
pub(super) use preview::SplitChoice;

use super::body::{Carried, Host};
use super::search::Marking;
use super::search::crlf::CrlfOffsets;
use super::search::memory::{MemoryLink, attach_memory};
use super::search::overview::Overview;
use super::{FileStamp, FileText};
use crate::prelude::*;
use crate::settings::{Field, Values};
use crate::tokens::Mode;
use find::EditorFind;
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
    stamp: Option<FileStamp>,
    persists: bool,
    dirty: bool,
    revision: u64,
    overwrite: bool,
    saving: Option<Task<()>>,
    note: Option<SharedString>,
    find: EditorFind,
    go_to: Option<lines::GoToBar>,
    marking: Marking,
    line_numbers: bool,
    gutter: Option<bool>,
    decorations: RangeDecorationCollection,
    decorated: Option<(u64, Mode)>,
    overview: Overview,
    recount: Option<Task<()>>,
    split: Option<Split>,
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

fn read_only_state(
    text: String,
    language: &'static str,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> EditorState {
    let mut state = EditorState::new(window, cx)
        .language(language)
        .default_value(text);
    state.set_readonly(true, cx);
    state.set_searchable(false, cx);
    state.set_context_menu_enabled(false);
    state
}

impl EditorView {
    fn new(opened: EditorFile, host: Host, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let EditorFile {
            file,
            language,
            split,
        } = opened;
        let shared: Arc<str> = Arc::from(file.text.as_str());
        let (persists, link) = memory_link(&file, &shared, &host);
        let note = file.read_only.then(|| read_only_note(&file));
        let editor = cx.new(|cx| read_only_state(file.text, language, window, cx));
        let decorations = editor.update(cx, |state, cx| {
            state.create_range_decorations_collection(Vec::new(), cx)
        });
        let split = split.map(|choice| Split::new(choice, host.clone(), &editor, cx));
        let changes = cx.subscribe(&editor, |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.text_changed(cx);
            }
        });
        let mut view = Self {
            host,
            editor,
            editing: false,
            can_edit: !file.read_only,
            crlf: file.crlf,
            ends: file.ends.clone(),
            encoding: file.encoding,
            stamp: file.stamp,
            persists,
            dirty: false,
            revision: 0,
            overwrite: false,
            saving: None,
            note,
            find: EditorFind::default(),
            go_to: None,
            marking: Marking::new(text_source(shared)),
            line_numbers: true,
            gutter: None,
            decorations,
            decorated: None,
            overview: Overview::default(),
            recount: None,
            split,
            _changes: changes,
        };
        attach_memory(&mut view, link, cx);
        view
    }

    fn show_parts(&mut self, values: &Values, cx: &mut Context<Self>) {
        self.line_numbers = values.flag(Field::TextLineNumbers);
        self.marking.line_ends = values.flag(Field::TextLineEnds);
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
        self.revision += 1;
        self.marking.freeze_bookmarks();
        self.overwrite = false;
        self.note = None;
        self.text_edited(cx);
        self.preview_edited(cx);
        cx.notify();
    }

    fn start_editing(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.can_edit {
            return false;
        }
        if !self.editing {
            self.editing = true;
            self.find.bar = None;
            self.marking.find_all = None;
            self.editor.update(cx, |state, cx| {
                state.set_readonly(false, cx);
                state.set_searchable(true, cx);
            });
            self.refresh_preview(cx);
            cx.notify();
        }
        true
    }
}
