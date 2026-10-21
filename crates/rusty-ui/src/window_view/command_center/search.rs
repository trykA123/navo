use std::sync::Arc;
use std::time::Duration;

use super::super::FerriteWindow;
use crate::command_center::{
    Cancel, CommandsSnapshot, Group, Hit, HitSink, Notes, Query, RecentSource, Request,
    ResultSource, Results, Row, Scope,
};
use crate::commands::ArgKind;
use crate::prelude::*;
use gpui_kit::{AsyncWindowContext, WeakEntity};

const LIMIT: usize = 500;
const RECENT_COMMANDS: usize = 5;
const RECENT_FOLDERS: usize = 8;
const FILE_DELAY: Duration = Duration::from_millis(40);

pub(super) fn recent(commands: Vec<Hit>, folders: Vec<Hit>) -> Vec<Row> {
    Results::recent(commands, folders)
}

pub(super) struct Search {
    recent: Arc<RecentSource>,
    commands: Arc<CommandsSnapshot>,
    sources: Arc<[Arc<dyn ResultSource>]>,
    generation: u64,
    cancel: Cancel,
    sink: HitSink,
    pub(super) shown: u64,
    pub(super) settled: u64,
}

impl Search {
    pub(super) fn new(
        sources: Vec<Arc<dyn ResultSource>>,
        (recent, commands): (Arc<RecentSource>, Arc<CommandsSnapshot>),
        generation: u64,
    ) -> Self {
        Self {
            recent,
            commands,
            sources: sources.into(),
            generation,
            cancel: Cancel::new(),
            sink: HitSink::new(generation),
            shown: generation,
            settled: generation,
        }
    }

    pub(super) fn cancel(&self) {
        self.cancel.cancel();
    }

    fn next(&mut self) -> u64 {
        self.cancel.cancel();
        self.cancel = Cancel::new();
        self.generation += 1;
        self.sink.begin(self.generation);
        self.generation
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    fn sources_for(&self, is_step: bool) -> Arc<[Arc<dyn ResultSource>]> {
        if !is_step {
            return self.sources.clone();
        }
        self.sources
            .iter()
            .filter(|source| source.id() == "paths")
            .cloned()
            .collect()
    }
}

fn in_path_mode(text: &str) -> Query {
    Query {
        scope: Scope::Files,
        terms: Vec::new(),
        path: Some(text.to_owned()),
    }
}

#[derive(Clone)]
struct Round {
    sources: Arc<[Arc<dyn ResultSource>]>,
    request: Arc<Request>,
    sink: HitSink,
    cancel: Cancel,
}

fn is_fast(source: &Arc<dyn ResultSource>) -> bool {
    source.group() == Group::Commands
}

impl Round {
    fn active(&self, fast: bool) -> impl Iterator<Item = &Arc<dyn ResultSource>> {
        let scope = self.request.query.scope;
        self.sources
            .iter()
            .filter(move |source| source.scopes().contains(scope) && is_fast(source) == fast)
    }

    fn run(&self, fast: bool) -> (Vec<Hit>, Notes) {
        for source in self.active(fast) {
            if self.cancel.is_cancelled() {
                break;
            }
            source.search(&self.request, &self.sink, &self.cancel);
        }
        let notes = if fast { Notes::default() } else { self.notes() };
        (self.sink.drain(self.request.generation), notes)
    }

    fn notes(&self) -> Notes {
        Notes {
            capped: self
                .active(false)
                .find_map(|source| source.capped(&self.request)),
            offline: self
                .active(false)
                .find_map(|source| source.offline(&self.request)),
        }
    }

    fn wait_for_more(&self) -> bool {
        self.active(false)
            .any(|source| source.wait_for_more(&self.request, &self.cancel))
    }
}

pub(in crate::window_view) struct Update {
    pub(in crate::window_view) generation: u64,
    pub(in crate::window_view) scope: Scope,
    pub(in crate::window_view) hits: Vec<Hit>,
    pub(in crate::window_view) notes: Notes,
    pub(in crate::window_view) settled: bool,
}

struct Stream {
    round: Round,
    scope: Scope,
    this: WeakEntity<FerriteWindow>,
}

impl Stream {
    async fn publish(
        &self,
        (hits, notes): (Vec<Hit>, Notes),
        settled: bool,
        cx: &mut AsyncWindowContext,
    ) -> bool {
        if self.round.cancel.is_cancelled() {
            return false;
        }
        let update = Update {
            generation: self.round.request.generation,
            scope: self.scope,
            hits,
            notes,
            settled,
        };
        self.this
            .update_in(cx, |view, window, cx| view.receive_hits(update, window, cx))
            .is_ok()
    }

    async fn run(&self, fast: bool, cx: &mut AsyncWindowContext) -> (Vec<Hit>, Notes) {
        let step = self.round.clone();
        cx.background_executor()
            .spawn(async move { step.run(fast) })
            .await
    }

    async fn go(self, cx: &mut AsyncWindowContext) {
        let (fast, _) = self.run(true, cx).await;
        let has_files = self.round.active(false).next().is_some();
        if (!fast.is_empty() || !has_files)
            && !self
                .publish((fast.clone(), Notes::default()), !has_files, cx)
                .await
        {
            return;
        }
        if !has_files {
            return;
        }
        cx.background_executor().timer(FILE_DELAY).await;
        loop {
            let (files, notes) = self.run(false, cx).await;
            let hits = fast.iter().cloned().chain(files).collect();
            if !self.publish((hits, notes), true, cx).await {
                return;
            }
            let step = self.round.clone();
            let more = cx
                .background_executor()
                .spawn(async move { step.wait_for_more() });
            if !more.await {
                return;
            }
        }
    }
}

impl FerriteWindow {
    pub(super) fn query_changed(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let here = Some(self.browser.state().active_tab().location().clone());
        let Some(open) = self.command_center.open.as_mut() else {
            return;
        };
        open.error = None;
        open.checking = None;
        open.run_when_shown = None;
        let generation = open.search.next();
        let step = open.step.as_ref().map(|step| step.kind);
        let is_blank = query.trim().is_empty();
        if step == Some(ArgKind::Text) || (step.is_some() && is_blank) {
            return self.show_rows((generation, Vec::new(), true), window, cx);
        }
        if is_blank {
            return self.load_recent(window, cx);
        }
        let query = if step.is_some() {
            in_path_mode(query)
        } else {
            Query::parse(query)
        };
        let scope = query.scope;
        let request = Request {
            query,
            generation,
            here,
            limit: LIMIT,
        };
        let search = &open.search;
        let round = Round {
            sources: search.sources_for(step.is_some()),
            request: Arc::new(request),
            sink: search.sink.clone(),
            cancel: search.cancel.clone(),
        };
        cx.spawn_in(window, async move |this, cx| {
            Stream { round, scope, this }.go(cx).await
        })
        .detach();
    }

    pub(super) fn load_recent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        let generation = open.search.generation();
        let source = open.search.recent.clone();
        let snapshot = open.search.commands.clone();
        let latest = self.command_center.hooks.recent_commands.clone();
        let task = cx.background_executor().spawn(async move {
            let commands = snapshot.hits_of(&latest(RECENT_COMMANDS));
            (commands, source.top_folders(RECENT_FOLDERS))
        });
        cx.spawn_in(window, async move |this, cx| {
            let (commands, folders) = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.show_rows((generation, recent(commands, folders), true), window, cx);
            });
        })
        .detach();
    }

    pub(in crate::window_view) fn receive_hits(
        &mut self,
        update: Update,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Update {
            generation,
            scope,
            hits,
            notes,
            settled,
        } = update;
        let rows = Results::with_cap(Results::merge(hits, scope), notes.capped);
        let rows = Results::with_offline(rows, notes.offline);
        self.show_rows((generation, rows, settled), window, cx);
    }

    fn show_rows(
        &mut self,
        (generation, rows, settled): (u64, Vec<Row>, bool),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.command_center.open.as_mut() else {
            return;
        };
        if generation != open.search.generation() {
            return;
        }
        open.selection = open.kept_selection(generation, &rows);
        open.search.shown = generation;
        open.rows = rows.into();
        let run = if settled {
            open.search.settled = generation;
            open.run_when_shown.take()
        } else {
            None
        };
        self.push_command_center(window, cx);
        if let Some(way) = run {
            self.run_selected(way, window, cx);
        }
    }
}
