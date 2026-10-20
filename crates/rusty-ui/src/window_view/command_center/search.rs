use std::sync::Arc;

use super::super::FerriteWindow;
use crate::command_center::{
    Cancel, CommandsSnapshot, Hit, HitSink, Query, RecentSource, Request, ResultSource, Results,
    Row, Scope, Selection,
};
use crate::prelude::*;

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

fn run_sources(
    sources: &[Arc<dyn ResultSource>],
    request: &Request,
    sink: &HitSink,
    cancel: &Cancel,
) {
    let scope = request.query.scope;
    for source in sources
        .iter()
        .filter(|source| source.scopes().contains(scope))
    {
        if cancel.is_cancelled() {
            return;
        }
        source.search(request, sink, cancel);
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
        let (sources, sink, cancel) = (
            search.sources.clone(),
            search.sink.clone(),
            search.cancel.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            run_sources(&sources, &request, &sink, &cancel);
            sink.drain(generation)
        });
        cx.spawn_in(window, async move |this, cx| {
            let hits = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.receive_hits((generation, scope, hits), window, cx);
            });
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
        (generation, scope, hits): (u64, Scope, Vec<Hit>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_rows((generation, Results::merge(hits, scope)), window, cx);
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
        open.selection = Selection::start(&rows);
        open.rows = rows.into();
        self.push_command_center(window, cx);
    }
}
