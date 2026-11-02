use std::rc::Rc;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{EventEmitter, FocusHandle, Focusable, Keystroke, Subscription};

use super::model::{Line, Row, lines, rows};
use super::proposal::{Conflict, Edit, Proposal, Recorded, conflict, edits, recorded};
use super::{SaveKeymap, Saved, view};
use crate::commands::{CommandSpec, Keymap, active};
use crate::prelude::*;
use crate::primitives::{ListScroll, TextFieldState, text_field_state};

const PLACEHOLDER: &str = "Search by name or key, like ctrl+f5";
const BLOCKED: &str = "keymap.toml has an error; fix it before changing keys.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShortcutsEvent {
    Saved(Saved),
    OpenFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Pending {
    pub(super) spec: CommandSpec,
    pub(super) keys: Option<Vec<String>>,
    edits: Vec<Edit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Browse,
    Recording(CommandSpec),
    Conflict(Pending, Conflict),
    Saving(Pending),
    Failed(Pending, SharedString),
}

impl Stage {
    pub(super) fn pending(&self) -> Option<&Pending> {
        match self {
            Self::Conflict(pending, _) | Self::Saving(pending) | Self::Failed(pending, _) => {
                Some(pending)
            }
            Self::Browse | Self::Recording(_) => None,
        }
    }
}

pub struct ShortcutsPage {
    save: SaveKeymap,
    pub(super) search: Entity<TextFieldState>,
    pub(super) scroll: ListScroll,
    query: String,
    built: Option<Keymap>,
    pub(super) rows: Rc<[Row]>,
    pub(super) lines: Rc<[Line]>,
    pub(super) selected: Option<&'static str>,
    pub(super) stage: Stage,
    pub(super) problem: Option<SharedString>,
    _search: Subscription,
    _keys: Subscription,
    _blur: Subscription,
}

impl EventEmitter<ShortcutsEvent> for ShortcutsPage {}

impl Focusable for ShortcutsPage {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search.focus_handle(cx)
    }
}

impl ShortcutsPage {
    pub fn new(save: SaveKeymap, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = text_field_state(PLACEHOLDER, window, cx);
        let on_search = cx.subscribe(&search, |page: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                page.query_changed(cx);
            }
        });
        let this = cx.weak_entity();
        let on_keys = cx.intercept_keystrokes(move |event, window, cx| {
            let stroke = &event.keystroke;
            let is_taken = this
                .update(cx, |page, cx| page.intercept(stroke, window, cx))
                .unwrap_or(false);
            if is_taken {
                cx.stop_propagation();
            }
        });
        let search_focus = search.focus_handle(cx);
        let on_blur = cx.on_blur(&search_focus, window, |page: &mut Self, _, cx| {
            page.stop_recording(cx);
        });
        let mut page = Self {
            save,
            search,
            scroll: ListScroll::new(),
            query: String::new(),
            built: None,
            rows: Rc::from([]),
            lines: Rc::from([]),
            selected: None,
            stage: Stage::Browse,
            problem: None,
            _search: on_search,
            _keys: on_keys,
            _blur: on_blur,
        };
        page.refresh(cx);
        page
    }

    pub fn set_problem(&mut self, problem: Option<SharedString>, cx: &mut Context<Self>) {
        let is_blocked = problem.is_some();
        self.problem = problem;
        if is_blocked {
            self.pause(cx);
        }
        cx.notify();
    }

    pub fn pause(&mut self, cx: &mut Context<Self>) {
        if matches!(self.stage, Stage::Recording(_) | Stage::Conflict(..)) {
            self.stage = Stage::Browse;
        }
        cx.notify();
    }

    fn stop_recording(&mut self, cx: &mut Context<Self>) {
        if self.is_recording() {
            self.stage = Stage::Browse;
            cx.notify();
        }
    }

    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = query.to_owned();
        self.search
            .update(cx, |state, cx| state.set_value(text, window, cx));
        self.query_changed(cx);
    }

    pub fn select(&mut self, command: &str, cx: &mut Context<Self>) {
        self.refresh(cx);
        let found = self.rows.iter().find(|row| row.spec.name == command);
        if let Some(row) = found {
            self.selected = Some(row.spec.name);
            self.scroll_to_selected();
        }
        cx.notify();
    }

    pub fn start_recording(&mut self, cx: &mut Context<Self>) {
        let spec = self.selected_row().map(|row| row.spec);
        if let (Stage::Browse, None, Some(spec)) = (&self.stage, &self.problem, spec) {
            self.stage = Stage::Recording(spec);
            cx.notify();
        }
    }

    pub fn take(&mut self, recorded: Recorded, cx: &mut Context<Self>) {
        let Stage::Recording(spec) = self.stage else {
            return;
        };
        let keys = match recorded {
            Recorded::Cancel => {
                self.stage = Stage::Browse;
                cx.notify();
                return;
            }
            Recorded::Unbind => Vec::new(),
            Recorded::Key(key) => vec![key],
        };
        let keymap = active(cx);
        let proposal = Proposal { spec, keys };
        let pending = Pending {
            spec,
            edits: edits(keymap, &proposal),
            keys: Some(proposal.keys.clone()),
        };
        match conflict(keymap, &proposal) {
            Some(found) => self.stage = Stage::Conflict(pending, found),
            None => self.write(pending, cx),
        }
        cx.notify();
    }

    pub fn replace(&mut self, cx: &mut Context<Self>) {
        if let Stage::Conflict(pending, _) | Stage::Failed(pending, _) = &self.stage {
            let pending = pending.clone();
            self.write(pending, cx);
        }
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.stage, Stage::Saving(_)) {
            self.stage = Stage::Browse;
            cx.notify();
        }
    }

    pub fn reset(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row().filter(|row| row.is_changed) else {
            return;
        };
        if self.problem.is_some() || self.stage != Stage::Browse {
            return;
        }
        let pending = Pending {
            spec: row.spec,
            keys: None,
            edits: vec![Edit::Reset {
                command: row.spec.name,
            }],
        };
        self.write(pending, cx);
    }

    #[must_use]
    pub fn is_recording(&self) -> bool {
        matches!(self.stage, Stage::Recording(_))
    }

    pub(super) fn open_file(&mut self, cx: &mut Context<Self>) {
        cx.emit(ShortcutsEvent::OpenFile);
    }

    pub(super) fn confirm(&mut self, cx: &mut Context<Self>) {
        match self.stage {
            Stage::Browse => self.start_recording(cx),
            Stage::Conflict(..) | Stage::Failed(..) => self.replace(cx),
            Stage::Recording(_) | Stage::Saving(_) => {}
        }
    }

    pub(super) fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if !matches!(self.stage, Stage::Browse) {
            return;
        }
        let at = self.selected_line();
        let named = self.named_lines();
        let next = if forward {
            named
                .iter()
                .find(|(line, _)| at.is_none_or(|at| *line > at))
        } else {
            named
                .iter()
                .rev()
                .find(|(line, _)| at.is_none_or(|at| *line < at))
        };
        if let Some(&(_, name)) = next {
            self.selected = Some(name);
            self.scroll_to_selected();
            cx.notify();
        }
    }

    pub(super) fn refresh(&mut self, cx: &App) {
        let keymap = active(cx);
        if self.built.as_ref() == Some(keymap) {
            return;
        }
        self.built = Some(keymap.clone());
        self.rows = rows(keymap).into();
        self.relines(false);
    }

    pub(super) fn selected_line(&self) -> Option<usize> {
        let name = self.selected?;
        self.lines.iter().position(|line| match line {
            Line::Row(row) => self.rows.get(*row).is_some_and(|row| row.spec.name == name),
            Line::Area(_) => false,
        })
    }

    fn selected_row(&self) -> Option<&Row> {
        let name = self.selected?;
        self.selected_line()?;
        self.rows.iter().find(|row| row.spec.name == name)
    }

    fn named_lines(&self) -> Vec<(usize, &'static str)> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| match line {
                Line::Row(row) => self.rows.get(*row).map(|row| (index, row.spec.name)),
                Line::Area(_) => None,
            })
            .collect()
    }

    fn first_row(&self) -> Option<&'static str> {
        self.named_lines().first().map(|(_, name)| *name)
    }

    fn relines(&mut self, to_first: bool) {
        self.lines = lines(&self.rows, &self.query).into();
        if to_first || self.selected_line().is_none() {
            self.selected = self.first_row();
        }
    }

    fn scroll_to_selected(&self) {
        if let Some(line) = self.selected_line() {
            self.scroll.scroll_to(line);
        }
    }

    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().to_string();
        if query == self.query {
            return;
        }
        self.query = query;
        self.refresh(cx);
        self.relines(true);
        self.scroll.scroll_to_top(0);
        cx.notify();
    }

    fn intercept(&mut self, stroke: &Keystroke, window: &Window, cx: &mut Context<Self>) -> bool {
        let is_here = self.search.focus_handle(cx).is_focused(window);
        if !self.is_recording() || !is_here {
            return false;
        }
        let Some(taken) = recorded(stroke) else {
            return false;
        };
        self.take(taken, cx);
        true
    }

    fn write(&mut self, pending: Pending, cx: &mut Context<Self>) {
        if self.problem.is_some() {
            self.stage = Stage::Failed(pending, BLOCKED.into());
            cx.notify();
            return;
        }
        self.stage = Stage::Saving(pending.clone());
        let save = self.save.clone();
        let edits = pending.edits;
        let task = cx.background_executor().spawn(async move { save(&edits) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |page, cx| page.saved(result, cx));
        })
        .detach();
        cx.notify();
    }

    fn saved(&mut self, result: Result<Saved, String>, cx: &mut Context<Self>) {
        let Stage::Saving(pending) = &self.stage else {
            return;
        };
        match result {
            Ok(saved) => {
                self.stage = Stage::Browse;
                cx.emit(ShortcutsEvent::Saved(saved));
            }
            Err(error) => self.stage = Stage::Failed(pending.clone(), error.into()),
        }
        cx.notify();
    }
}

#[cfg(test)]
impl ShortcutsPage {
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn visible(&self) -> Vec<&'static str> {
        self.named_lines()
            .into_iter()
            .map(|(_, name)| name)
            .collect()
    }
}

impl Render for ShortcutsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh(cx);
        view::render(self, window, cx)
    }
}
