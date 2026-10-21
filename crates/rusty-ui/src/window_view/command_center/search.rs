use std::sync::Arc;

use super::super::FerriteWindow;
use crate::command_center::{
    Cancel, Capped, CommandsSnapshot, Hit, HitSink, Query, RecentSource, Request, ResultSource,
    Results, Row, Scope,
};
use crate::prelude::*;
use gpui_kit::{AsyncWindowContext, WeakEntity};

const LIMIT: usize = 500;
const RECENT_COMMANDS: usize = 5;
const RECENT_FOLDERS: usize = 8;

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
}

impl Search {
    pub(super) fn new(
        sources: Vec<Arc<dyn ResultSource>>,
        (recent, commands): (Arc<RecentSource>, Arc<CommandsSnapshot>),
    ) -> Self {
        Self {
            recent,
            commands,
            sources: sources.into(),
            generation: 0,
            cancel: Cancel::new(),
            sink: HitSink::new(0),
            shown: 0,
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
}

#[derive(Clone)]
struct Round {
    sources: Arc<[Arc<dyn ResultSource>]>,
    request: Arc<Request>,
    sink: HitSink,
    cancel: Cancel,
}

impl Round {
    fn active(&self) -> impl Iterator<Item = &Arc<dyn ResultSource>> {
        let scope = self.request.query.scope;
        self.sources
            .iter()
            .filter(move |source| source.scopes().contains(scope))
    }

    fn run(&self) -> (Vec<Hit>, Option<Capped>) {
        for source in self.active() {
            if self.cancel.is_cancelled() {
                break;
            }
            source.search(&self.request, &self.sink, &self.cancel);
        }
        let capped = self
            .active()
            .find_map(|source| source.capped(&self.request));
        (self.sink.drain(self.request.generation), capped)
    }

    fn wait_for_more(&self) -> bool {
        self.active()
            .any(|source| source.wait_for_more(&self.request, &self.cancel))
    }
}

async fn stream(
    round: Round,
    scope: Scope,
    this: WeakEntity<FerriteWindow>,
    cx: &mut AsyncWindowContext,
) {
    let generation = round.request.generation;
    loop {
        let step = round.clone();
        let (hits, capped) = cx
            .background_executor()
            .spawn(async move { step.run() })
            .await;
        let update = (generation, scope, hits, capped);
        let shown = this.update_in(cx, |view, window, cx| view.receive_hits(update, window, cx));
        let step = round.clone();
        let more = cx
            .background_executor()
            .spawn(async move { step.wait_for_more() });
        if shown.is_err() || !more.await {
            return;
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
        let generation = open.search.next();
        if open.step.is_some() || query.trim().is_empty() {
            if open.step.is_some() {
                return self.show_rows((generation, Vec::new()), window, cx);
            }
            return self.load_recent(window, cx);
        }
        let query = Query::parse(query);
        let scope = query.scope;
        let request = Request {
            query,
            generation,
            here,
            limit: LIMIT,
        };
        let search = &open.search;
        let round = Round {
            sources: search.sources.clone(),
            request: Arc::new(request),
            sink: search.sink.clone(),
            cancel: search.cancel.clone(),
        };
        cx.spawn_in(window, async move |this, cx| {
            stream(round, scope, this, cx).await
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
                view.show_rows((generation, recent(commands, folders)), window, cx);
            });
        })
        .detach();
    }

    pub(in crate::window_view) fn receive_hits(
        &mut self,
        (generation, scope, hits, capped): (u64, Scope, Vec<Hit>, Option<Capped>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = Results::with_cap(Results::merge(hits, scope), capped);
        self.show_rows((generation, rows), window, cx);
    }

    fn show_rows(
        &mut self,
        (generation, rows): (u64, Vec<Row>),
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
        self.push_command_center(window, cx);
    }
}
