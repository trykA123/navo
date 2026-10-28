mod work;

use std::rc::Rc;

use chrono::{Local, Utc};
use gpui_kit::FocusHandle;
use rusty_core::retention::RetentionId;
use rusty_l10n::t;

use super::FerriteWindow;
use crate::browser::{RetainedHooks, RetainedRow};
use crate::prelude::*;
use crate::primitives::ListScroll;
use crate::recently_deleted::{
    Event, MAX_ROWS, Model, PAGE_ROWS, Phase, RecentlyDeletedBody, SheetParts, sheet, summary,
};
use crate::side_sheet::SheetState;
use crate::tokens;

pub(super) struct RecentPanel {
    sheet: Entity<SheetState>,
    focus: FocusHandle,
    scroll: ListScroll,
    is_open: bool,
    restore_preview: bool,
    generation: u64,
    asks: u64,
    phase: Phase,
    rows: Vec<RetainedRow>,
    model: Rc<Model>,
    cursor: usize,
    working: Rc<Vec<RetentionId>>,
    status: Option<SharedString>,
    ask: Option<RetainedRow>,
}

impl RecentPanel {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            sheet: cx.new(|_| SheetState::new()),
            focus: cx.focus_handle(),
            scroll: ListScroll::new(),
            is_open: false,
            restore_preview: false,
            generation: 0,
            asks: 0,
            phase: Phase::Loading,
            rows: Vec::new(),
            model: Rc::default(),
            cursor: 0,
            working: Rc::default(),
            status: None,
            ask: None,
        }
    }

    fn current(&self) -> Option<&RetainedRow> {
        self.model.row_at(self.cursor)
    }

    fn neighbour(&self) -> Option<RetentionId> {
        [true, false].into_iter().find_map(|forward| {
            let at = self.model.step(self.cursor, forward);
            let row = self.model.row_at(at).filter(|_| at != self.cursor);
            row.map(|row| row.id)
        })
    }

    fn rebuild(&mut self, keep: Option<RetentionId>) {
        let now = Utc::now().timestamp_millis();
        self.model = Rc::new(Model::new(&self.rows, now, &Local));
        self.cursor = keep
            .and_then(|id| self.model.index_of(id))
            .or_else(|| self.model.first_item())
            .unwrap_or(0);
    }

    fn drop_row(&mut self, id: RetentionId) {
        let keep = match self.current() {
            Some(row) if row.id == id => self.neighbour(),
            Some(row) => Some(row.id),
            None => None,
        };
        self.rows.retain(|row| row.id != id);
        self.rebuild(keep);
    }

    fn end_work(&mut self, id: RetentionId) {
        Rc::make_mut(&mut self.working).retain(|busy| *busy != id);
    }
}

const MAX_PAGES: usize = 250;

pub fn load_all(hooks: &dyn RetainedHooks) -> Result<Vec<RetainedRow>, String> {
    let mut rows: Vec<RetainedRow> = Vec::new();
    let mut after = None;
    for _ in 0..MAX_PAGES {
        let page = hooks.list(after, PAGE_ROWS)?;
        let is_short = page.len() < usize::from(PAGE_ROWS);
        after = page.last().map(|row| row.id);
        rows.extend(page);
        if rows.len() > MAX_ROWS {
            rows.drain(..rows.len() - MAX_ROWS);
        }
        if is_short {
            break;
        }
    }
    Ok(rows)
}

impl FerriteWindow {
    pub(super) fn open_recently_deleted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.recent.is_open {
            self.focus_recent(window, cx);
            return;
        }
        let is_busy = self.prompt.is_some()
            || self.browser.pending().is_some()
            || self.is_multi_rename_open()
            || self.settings.is_open()
            || self.viewer.is_some();
        if is_busy {
            return;
        }
        if self.app_menu.is_some() {
            self.close_app_menu(window, cx);
        }
        self.drive_menu = None;
        self.path_edit = None;
        if self.preview.is_open() {
            self.close_preview(cx);
            self.recent.restore_preview = true;
        }
        self.recent.is_open = true;
        self.recent.phase = Phase::Loading;
        self.recent.rows.clear();
        self.recent.status = None;
        self.recent.rebuild(None);
        self.recent.sheet.update(cx, |sheet, cx| sheet.open(cx));
        self.focus_recent(window, cx);
        self.load_retained(cx);
        cx.notify();
    }

    pub(super) fn close_recently_deleted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.recent.is_open {
            return;
        }
        self.recent.is_open = false;
        self.recent.ask = None;
        self.recent.generation += 1;
        self.recent.sheet.update(cx, |sheet, cx| sheet.close(cx));
        if std::mem::take(&mut self.recent.restore_preview) {
            self.open_preview(window, cx);
        }
        self.focus_core_pane(window, cx);
        cx.notify();
    }

    fn focus_recent(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.recent.focus, cx);
    }

    fn load_retained(&mut self, cx: &mut Context<Self>) {
        self.recent.generation += 1;
        let generation = self.recent.generation;
        let hooks = self.browser.retained_hooks();
        let loading = cx.background_spawn(async move { load_all(hooks.as_ref()) });
        cx.spawn(async move |this, cx| {
            let outcome = loading.await;
            let _ = this.update(cx, |view, cx| view.retained_listed(generation, outcome, cx));
        })
        .detach();
    }

    fn retained_listed(
        &mut self,
        generation: u64,
        outcome: Result<Vec<RetainedRow>, String>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.recent.generation || !self.recent.is_open {
            return;
        }
        let keep = self.recent.current().map(|row| row.id);
        match outcome {
            Ok(rows) => {
                self.recent.rows = rows;
                self.recent.phase = Phase::Ready;
                self.recent.rebuild(keep);
            }
            Err(error) => {
                let failed = t!("retained-list-failed", error = error);
                self.recent.phase = Phase::Failed(failed.into());
            }
        }
        cx.notify();
    }

    pub(super) fn recent_event(
        &mut self,
        event: Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            Event::Move(forward) => {
                self.recent.cursor = self.recent.model.step(self.recent.cursor, forward);
                self.recent.scroll.scroll_to(self.recent.cursor);
            }
            Event::Select(at) if self.recent.model.row_at(at).is_some() => {
                self.recent.cursor = at;
            }
            Event::Select(_) => {}
            Event::Restore => self.restore_current(cx),
            Event::Delete => self.ask_purge_current(),
            Event::Close => self.close_recently_deleted(window, cx),
        }
        cx.notify();
    }

    pub(super) fn recent_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.viewer.is_some() || !self.recent.sheet.read(cx).is_shown(cx) {
            return None;
        }
        let t = tokens::current(cx).clone();
        let frame = self.sheet_frame(&t, window, cx).covering_passive_card();
        let weak = cx.weak_entity();
        let body = RecentlyDeletedBody::new(
            &self.recent.model,
            self.recent.phase.clone(),
            &self.recent.scroll,
        )
        .cursor(self.recent.cursor)
        .working(&self.recent.working)
        .now_ms(Utc::now().timestamp_millis())
        .track_focus(&self.recent.focus)
        .on_event(move |event, window, cx| {
            let _ = weak.update(cx, |view, cx| view.recent_event(event, window, cx));
        });
        let model = &self.recent.model;
        let parts = SheetParts {
            subtitle: if model.is_empty() {
                SharedString::default()
            } else {
                summary(model.items(), model.bytes()).into()
            },
            status: self.recent.status.clone(),
            has_rows: !model.is_empty(),
        };
        Some(
            div()
                .absolute()
                .top(t.spacing.xs)
                .bottom(t.spacing.xs)
                .left(t.spacing.md)
                .right(t.spacing.md)
                .child(sheet(&self.recent.sheet, frame, body, parts))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
pub(super) struct RecentView {
    pub is_open: bool,
    pub items: Vec<i64>,
    pub cursor: Option<i64>,
    pub status: Option<String>,
    pub is_asking: bool,
    pub working: usize,
}

#[cfg(test)]
impl FerriteWindow {
    pub(super) fn recent_view(&self) -> RecentView {
        let recent = &self.recent;
        let shown = (0..recent.model.len()).filter_map(|at| recent.model.row_at(at));
        RecentView {
            is_open: recent.is_open,
            items: shown.map(|row| row.id.0).collect(),
            cursor: recent.current().map(|row| row.id.0),
            status: recent.status.as_ref().map(ToString::to_string),
            is_asking: recent.ask.is_some(),
            working: recent.working.len(),
        }
    }
}
