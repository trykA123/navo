mod body;
mod card;
mod chapters;
mod doc;
mod find;
mod follow;
mod hit_list;
mod hits;
mod jobs;
mod keys;
mod layout;
mod links;
mod look;
mod navigate;
mod outline;
mod page_list;
mod rail;
mod ring;
mod search;
mod select;
mod sheets;
mod shots;
mod source;
mod stage;
mod text;
mod toolbar;
mod trail;
mod trouble;
mod view;
mod work;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{FocusHandle, Focusable, Subscription, Task};
use rusty_core::Location;
use rusty_pdf::protocol::PdfError;

pub(super) use body::PdfBody;
pub use keys::bind_keys;
pub use look::PdfLook;
pub(super) use source::no_pdf;
pub use source::{OpenPdf, PdfPages};

use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};
use crate::side_sheet::{SheetState, Side};
use doc::Doc;
use search::Query;
use stage::Measured;

pub(super) const PDF_CONTEXT: &str = "Pdf";

pub struct PdfStart {
    pub title: SharedString,
    pub len: u64,
    pub location: Location,
    pub open: OpenPdf,
    pub opened: Result<Box<dyn PdfPages>, PdfError>,
    pub is_preview: bool,
}

enum Phase {
    Locked { error: Option<SharedString> },
    Unlocking { _task: Task<()> },
    Refused(PdfError),
    Ready(Box<Doc>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeftTab {
    Pages,
    Outline,
}

struct Inputs {
    pub find: Entity<TextFieldState>,
    pub page: Entity<TextFieldState>,
    pub password: Entity<TextFieldState>,
    _events: [Subscription; 3],
}

pub struct PdfView {
    title: SharedString,
    len: u64,
    location: Location,
    open: OpenPdf,
    phase: Phase,
    focus: FocusHandle,
    pages_focus: FocusHandle,
    hits_focus: FocusHandle,
    inputs: Inputs,
    query: Query,
    searching: Option<Task<()>>,
    is_finding: bool,
    is_preview: bool,
    is_active: bool,
    _focus_in: Subscription,
    pages_with_hits: bool,
    look: PdfLook,
    left: Entity<SheetState>,
    right: Entity<SheetState>,
    left_tab: LeftTab,
    drawers: Vec<Side>,
    note: Option<SharedString>,
    measured: Rc<Cell<Measured>>,
}

fn phase_of(opened: Result<Box<dyn PdfPages>, PdfError>, is_preview: bool) -> Phase {
    match opened {
        Ok(pages) => Phase::Ready(Box::new(Doc::new(pages, is_preview))),
        Err(PdfError::NeedsPassword) => Phase::Locked { error: None },
        Err(error) => Phase::Refused(error),
    }
}

impl PdfView {
    pub fn new(start: PdfStart, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let inputs = Inputs::new(window, cx);
        cx.on_release(|view: &mut Self, cx| {
            let images = view.doc_mut().map(Doc::drain_images).unwrap_or_default();
            for image in images {
                crate::primitives::release_image(image, cx);
            }
        })
        .detach();
        let focus = cx.focus_handle();
        let focus_in = cx.on_focus_in(&focus, window, |view: &mut Self, _, cx| {
            if !view.is_active {
                view.is_active = true;
                cx.notify();
            }
        });
        Self {
            title: start.title,
            len: start.len,
            location: start.location,
            open: start.open,
            phase: phase_of(start.opened, start.is_preview),
            focus,
            pages_focus: cx.focus_handle(),
            hits_focus: cx.focus_handle(),
            inputs,
            query: Query::default(),
            searching: None,
            is_finding: false,
            is_preview: start.is_preview,
            is_active: !start.is_preview,
            _focus_in: focus_in,
            pages_with_hits: false,
            look: PdfLook::default(),
            left: cx.new(|_| SheetState::new()),
            right: cx.new(|_| SheetState::new()),
            left_tab: LeftTab::Pages,
            drawers: Vec::new(),
            note: None,
            measured: Rc::default(),
        }
    }

    pub fn set_look(&mut self, look: PdfLook, cx: &mut Context<Self>) {
        self.look = look;
        cx.notify();
    }

    fn doc(&self) -> Option<&Doc> {
        match &self.phase {
            Phase::Ready(doc) => Some(doc),
            _ => None,
        }
    }

    fn doc_mut(&mut self) -> Option<&mut Doc> {
        match &mut self.phase {
            Phase::Ready(doc) => Some(doc),
            _ => None,
        }
    }

    fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }

    fn focus_pages(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }
}

impl Inputs {
    fn new(window: &mut Window, cx: &mut Context<PdfView>) -> Self {
        let find = text_field_state("Text to find", window, cx);
        let page = text_field_state("Page", window, cx);
        let password = cx.new(|cx| {
            TextFieldState::new(window, cx)
                .placeholder("Password")
                .masked(true)
        });
        let events = [
            cx.subscribe_in(
                &find,
                window,
                |view, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { shift, .. } => view.step(*shift, window, cx),
                    InputEvent::Change => view.typed_query(window, cx),
                    InputEvent::Focus => view.start_finding(cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(&page, window, |view, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    view.submit_page(window, cx);
                }
            }),
            cx.subscribe_in(
                &password,
                window,
                |view, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        view.unlock(window, cx);
                    }
                },
            ),
        ];
        Self {
            find,
            page,
            password,
            _events: events,
        }
    }
}

impl Focusable for PdfView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
