mod keys;
mod open;
mod run;
mod search;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{Bounds, FocusHandle, Pixels, Subscription, point, px, size};

use super::{FerriteWindow, SearchHooks};
use crate::command_center::{
    Argument, CommandCenter, CommandCenterEvent, CommandCenterProps, CommandsSnapshot,
    CommandsSource, HereSource, HereWalks, HintKeys, INPUT_ROW, LEAVE, PathsSource, RecentSource,
    ResultSource, Row, Selection, TabsSnapshot, TabsSource,
};
use crate::commands::{self, ArgKind};
use crate::modifier_hints::HintsMode;
use crate::prelude::*;
use crate::tokens;

pub(super) type Anchor = Rc<Cell<Option<Bounds<Pixels>>>>;

pub(super) struct CommandCenterState {
    open: Option<Open>,
    leaving: Option<Entity<CommandCenter>>,
    pub(super) anchor: Anchor,
    pub(super) sources: Vec<Arc<dyn ResultSource>>,
    pub(super) hooks: SearchHooks,
    pub(super) visits: super::visits::Visits,
    pub(super) here: HereWalks,
    generation: u64,
}

impl CommandCenterState {
    pub(super) fn new(hooks: SearchHooks, provider: &Arc<dyn rusty_fs::ProviderOps>) -> Self {
        Self {
            open: None,
            leaving: None,
            anchor: Anchor::default(),
            sources: Vec::new(),
            hooks,
            visits: super::visits::Visits::default(),
            here: HereWalks::new(provider.clone()),
            generation: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    name: &'static str,
    kind: ArgKind,
    label: SharedString,
}

struct Open {
    center: Entity<CommandCenter>,
    rows: Rc<[Row]>,
    selection: Selection,
    step: Option<Step>,
    error: Option<SharedString>,
    anchor: Bounds<Pixels>,
    keys: HintKeys,
    restore: Option<FocusHandle>,
    search: search::Search,
    run_when_shown: Option<open::Way>,
    _events: Subscription,
}

impl Open {
    fn kept_selection(&self, generation: u64, rows: &[Row]) -> Selection {
        let start = Selection::start(rows);
        let moved = self.selection != Selection::start(&self.rows);
        if self.search.shown != generation || !moved {
            return start;
        }
        let target = |row: &Row| match row {
            Row::TopHit(hit) | Row::Hit(hit) => Some((hit.source, hit.target.clone())),
            _ => None,
        };
        let Some(chosen) = self.selection.row(&self.rows).and_then(target) else {
            return start;
        };
        rows.iter()
            .position(|row| target(row).as_ref() == Some(&chosen))
            .map_or(start, |at| Selection::at(rows, at))
    }

    fn props(&self, has_key_hints: bool) -> CommandCenterProps {
        CommandCenterProps {
            rows: self.rows.clone(),
            selection: self.selection,
            argument: self.step.as_ref().map(|step| Argument {
                label: step.label.clone(),
                kind: step.kind,
            }),
            note: None,
            error: self.error.clone(),
            anchor: self.anchor,
            key_hints: has_key_hints.then(|| self.keys.clone()),
            is_leaving: false,
        }
    }
}

fn focused_contexts(window: &Window) -> Vec<String> {
    window
        .context_stack()
        .iter()
        .filter_map(|context| context.primary().map(|entry| entry.key.to_string()))
        .collect()
}

impl FerriteWindow {
    pub(super) fn toggle_command_center(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_center.open.is_some() {
            return self.close_command_center(window, cx);
        }
        if self.prompt.is_some() || self.browser.pending().is_some() || self.is_multi_rename_open()
        {
            return;
        }
        if self.app_menu.is_some() {
            self.close_app_menu(window, cx);
        }
        self.open_command_center(window, cx);
    }

    fn file_sources(
        &self,
        snapshot: &Arc<CommandsSnapshot>,
    ) -> (Vec<Arc<dyn ResultSource>>, Arc<RecentSource>) {
        let hooks = &self.command_center.hooks;
        let mut sources: Vec<Arc<dyn ResultSource>> = vec![Arc::new(CommandsSource::with_use(
            snapshot.clone(),
            hooks.command_use.clone(),
        ))];
        let recent = Arc::new(RecentSource::new(hooks.frecent.clone()));
        sources.push(recent.clone());
        sources.push(Arc::new(TabsSource::new(TabsSnapshot::of(
            self.browser.state(),
        ))));
        sources.push(Arc::new(HereSource::new(self.command_center.here.clone())));
        sources.push(Arc::new(PathsSource::new(
            self.provider.clone(),
            Arc::new(|name| std::env::var(name).ok()),
        )));
        sources.extend(self.command_center.sources.iter().cloned());
        (sources, recent)
    }

    fn open_command_center(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restore = window.focused(cx);
        let stack = focused_contexts(window);
        let stack: Vec<&str> = stack.iter().map(String::as_str).collect();
        let keymap = commands::active(cx);
        let snapshot = CommandsSnapshot::build(&commands::entries(), &stack, keymap);
        let keys = HintKeys::of(keymap);
        let snapshot = Arc::new(snapshot);
        let (sources, recent) = self.file_sources(&snapshot);
        let generation = self.command_center.generation + 1;
        let rows: Rc<[Row]> = search::recent(Vec::new(), Vec::new()).into();
        let anchor = self.command_center_anchor(window, cx);
        let props = CommandCenterProps {
            rows: rows.clone(),
            selection: Selection::start(&rows),
            argument: None,
            note: None,
            error: None,
            anchor,
            key_hints: self.has_key_hints().then(|| keys.clone()),
            is_leaving: false,
        };
        let center = cx.new(|cx| CommandCenter::new(props, window, cx));
        let events = cx.subscribe_in(&center, window, |view, _, event, window, cx| {
            view.command_center_event(event, window, cx);
        });
        self.command_center.leaving = None;
        self.command_center.open = Some(Open {
            center,
            selection: Selection::start(&rows),
            rows,
            step: None,
            error: None,
            anchor,
            keys,
            restore,
            search: search::Search::new(sources, (recent, snapshot), generation),
            run_when_shown: None,
            _events: events,
        });
        self.load_recent(window, cx);
        cx.notify();
    }

    fn command_center_anchor(&self, window: &Window, cx: &App) -> Bounds<Pixels> {
        self.command_center.anchor.get().unwrap_or_else(|| {
            let top = tokens::current(cx).spacing.header_height;
            let width = window.viewport_size().width;
            Bounds::new(point(px(0.), top), size(width, px(INPUT_ROW)))
        })
    }

    fn has_key_hints(&self) -> bool {
        self.values.modifier_hints() != HintsMode::Off
    }

    pub(super) fn close_command_center(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.take() else {
            return;
        };
        open.search.cancel();
        self.command_center.generation = open.search.generation();
        self.command_center.here.cancel();
        match &open.restore {
            Some(handle) => window.focus(handle, cx),
            None => self.focus_core_pane(window, cx),
        }
        if !cx.reduce_motion() {
            self.leave(open, window, cx);
        }
        cx.notify();
    }

    fn leave(&mut self, open: Open, window: &mut Window, cx: &mut Context<Self>) {
        let props = CommandCenterProps {
            is_leaving: true,
            ..open.props(self.has_key_hints())
        };
        let center = open.center;
        center.update(cx, |center, cx| center.set_props(props, window, cx));
        self.command_center.leaving = Some(center.clone());
        let timer = cx.background_executor().timer(LEAVE);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |view, cx| view.forget_leaving(&center, cx));
        })
        .detach();
    }

    fn forget_leaving(&mut self, center: &Entity<CommandCenter>, cx: &mut Context<Self>) {
        let leaving = &mut self.command_center.leaving;
        if leaving.as_ref() == Some(center) {
            *leaving = None;
            cx.notify();
        }
    }

    fn command_center_event(
        &mut self,
        event: &CommandCenterEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            CommandCenterEvent::QueryChanged(query) => self.query_changed(query, window, cx),
            CommandCenterEvent::Moved(index) => {
                if let Some(open) = self.command_center.open.as_mut() {
                    open.selection = Selection::at(&open.rows, *index);
                }
                self.push_command_center(window, cx);
            }
            CommandCenterEvent::Run(index, modifiers) => {
                if let Some(open) = self.command_center.open.as_mut() {
                    open.selection = Selection::at(&open.rows, *index);
                }
                self.run_selected(open::Way::of_click(*modifiers), window, cx);
            }
            CommandCenterEvent::Dismissed => self.close_command_center(window, cx),
        }
    }

    fn push_command_center(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        let props = open.props(self.has_key_hints());
        open.center
            .update(cx, |center, cx| center.set_props(props, window, cx));
    }

    pub(super) fn command_center_view(&self) -> Option<Entity<CommandCenter>> {
        let open = self.command_center.open.as_ref();
        open.map(|open| open.center.clone())
            .or_else(|| self.command_center.leaving.clone())
    }
}

#[cfg(test)]
impl FerriteWindow {
    pub(super) fn command_center_selection(&self) -> Option<usize> {
        let open = self.command_center.open.as_ref()?;
        open.selection.index()
    }

    pub(super) fn command_center_generation(&self) -> Option<u64> {
        let open = self.command_center.open.as_ref()?;
        Some(open.search.generation())
    }
}
