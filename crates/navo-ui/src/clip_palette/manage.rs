//! The library view: F2 or "Manage…" swaps the palette body for the kinds on the left and the
//! rows of one kind on the right. It shares the palette's search field; Esc goes back.

use std::rc::Rc;

use gpui_kit::{AppContext as _, Context, Window};
use navo_core::Location;
use navo_l10n::tr;

use super::manage_kinds::{Answer, Ask, Kind, gather, offered};
use super::view::ClipPalette;
use super::{ClipId, ClipKind, ClipRow, Origin};
use crate::primitives::ListScroll;

/// The library's own state; the search text lives in the palette's field.
pub(super) struct Manage {
    kinds: Vec<Kind>,
    kind: Kind,
    rows: Rc<[ClipRow]>,
    counts: Vec<(Kind, usize)>,
    selected: usize,
    keep: Option<ClipId>,
    request: u64,
    is_loaded: bool,
    pub scroll: ListScroll,
}

impl Manage {
    /// None when no kind can be listed.
    pub fn new(kinds: Vec<Kind>) -> Option<Self> {
        let kind = *kinds.first()?;
        Some(Self {
            kinds,
            kind,
            rows: Rc::default(),
            counts: Vec::new(),
            selected: 0,
            keep: None,
            request: 0,
            is_loaded: false,
            scroll: ListScroll::new(),
        })
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn is_listed(&self, kind: Kind) -> bool {
        self.kinds.contains(&kind)
    }

    pub fn rows(&self) -> &Rc<[ClipRow]> {
        &self.rows
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_row(&self) -> Option<&ClipRow> {
        self.rows.get(self.selected)
    }

    pub fn is_loaded(&self) -> bool {
        self.is_loaded
    }

    /// Rows of the shown kind that match the search, or the count the last answer gave.
    pub fn count_of(&self, kind: Kind) -> Option<usize> {
        self.counts
            .iter()
            .find_map(|(listed, count)| (*listed == kind).then_some(*count))
    }

    /// Starts a request; a refresh keeps the selected row, a new search starts at the top.
    pub fn begin(
        &mut self,
        is_refresh: bool,
        query: String,
        folder: Option<Location>,
    ) -> (u64, Ask) {
        self.keep = is_refresh
            .then(|| self.selected_row().map(|row| row.id))
            .flatten();
        if !is_refresh {
            self.selected = 0;
        }
        self.request += 1;
        let ask = Ask {
            query,
            folder,
            kind: self.kind,
            kinds: self.kinds.clone(),
        };
        (self.request, ask)
    }

    /// Takes the answer to the latest request; false for an older one.
    pub fn accept(&mut self, request: u64, answer: Answer) -> bool {
        if request != self.request {
            return false;
        }
        self.rows = answer.rows.into();
        self.counts = answer.counts;
        self.is_loaded = true;
        let kept = self
            .keep
            .take()
            .and_then(|id| self.rows.iter().position(|row| row.id == id));
        self.selected = kept
            .unwrap_or(self.selected)
            .min(self.rows.len().saturating_sub(1));
        true
    }

    /// Moves to the next or previous listed kind; false when there is nowhere to go.
    pub fn cycle(&mut self, backwards: bool) -> bool {
        let count = self.kinds.len();
        let Some(at) = self.kinds.iter().position(|kind| *kind == self.kind) else {
            return false;
        };
        let to = if backwards {
            (at + count - 1) % count
        } else {
            (at + 1) % count
        };
        self.pick(self.kinds[to])
    }

    /// Picks a listed kind; false when it is not listed or already shown.
    pub fn pick(&mut self, kind: Kind) -> bool {
        if kind == self.kind || !self.is_listed(kind) {
            return false;
        }
        self.kind = kind;
        self.selected = 0;
        true
    }

    pub fn step(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
        self.scroll.scroll_to(self.selected);
    }

    pub fn select(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = index;
        }
    }
}

impl ClipPalette {
    /// Opens the library on the first kind the hooks list; nothing when it is open or none is.
    pub fn open_manage(&mut self, cx: &mut Context<Self>) {
        if self.manage.is_some() || self.form.is_some() {
            return;
        }
        let Some(manage) = Manage::new(offered(self.state.modes())) else {
            return;
        };
        self.overlay.menu = None;
        self.overlay.revealed = None;
        self.overlay.status = None;
        self.manage = Some(manage);
        self.refresh(false, cx);
        cx.notify();
    }

    /// Esc: back to the palette with the search as it is.
    pub(super) fn close_manage(&mut self, cx: &mut Context<Self>) {
        self.overlay.status = None;
        if self.manage.take().is_some() {
            self.refresh(true, cx);
            cx.notify();
        }
    }

    /// F2: edits the selected saved row, or offers to re-point a note whose folder is gone;
    /// on a history row or none it opens the library.
    pub fn edit_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.form.is_some() || self.repoint.is_some() {
            return;
        }
        let row = match &self.manage {
            Some(manage) => manage.selected_row(),
            None => self.state.selected_row(),
        };
        match row.cloned() {
            Some(row) if row.kind == ClipKind::Note => self.offer_repoint(&row, window, cx),
            Some(row) if row.from == Origin::Saved => self.open_edit(row.id, window, cx),
            _ => self.open_manage(cx),
        }
    }

    pub(super) fn refresh_manage(&mut self, is_refresh: bool, cx: &mut Context<Self>) {
        let query = self.query(cx).to_string();
        let Some(manage) = self.manage.as_mut() else {
            return;
        };
        let (request, ask) = manage.begin(is_refresh, query, self.folder.clone());
        let hooks = self.hooks.clone();
        let search = cx.background_spawn(async move { gather(hooks.as_ref(), &ask) });
        cx.spawn(async move |this, cx| {
            let answer = search.await;
            let _ = this.update(cx, |palette, cx| palette.take_manage(request, answer, cx));
        })
        .detach();
    }

    fn take_manage(&mut self, request: u64, answer: Answer, cx: &mut Context<Self>) {
        let Some(manage) = self.manage.as_mut() else {
            return;
        };
        if manage.accept(request, answer) {
            manage.scroll.scroll_to(manage.selected());
            cx.notify();
        }
    }

    pub(super) fn manage_cycle(&mut self, backwards: bool, cx: &mut Context<Self>) {
        if self.manage.as_mut().is_some_and(|m| m.cycle(backwards)) {
            self.refresh(false, cx);
            cx.notify();
        }
    }

    pub(super) fn pick_kind(&mut self, kind: Kind, cx: &mut Context<Self>) {
        if self.manage.as_mut().is_some_and(|m| m.pick(kind)) {
            self.refresh(false, cx);
            cx.notify();
        }
    }

    pub(super) fn manage_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(manage) = self.manage.as_mut() {
            manage.step(delta);
            cx.notify();
        }
    }

    pub(super) fn manage_select(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(manage) = self.manage.as_mut() {
            manage.select(index);
            cx.notify();
        }
    }

    /// Del: deletes the selected row through the hooks. The next snapshot drops it from the list.
    pub(super) fn manage_delete(&mut self, cx: &mut Context<Self>) {
        let Some((id, kind, scope)) = self
            .manage
            .as_ref()
            .and_then(|m| m.selected_row())
            .map(|row| (row.id, row.kind, row.scope.clone()))
        else {
            return;
        };
        match self.hooks.delete(id) {
            Ok(()) => {
                if kind == ClipKind::Note && self.note.holds(&scope) {
                    self.note.mark_deleted();
                }
                self.say(tr("clip-status-deleted"), false, cx);
            }
            Err(message) => self.say(message, true, cx),
        }
    }
}
