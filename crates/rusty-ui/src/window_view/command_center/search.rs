use std::sync::Arc;

use super::super::FerriteWindow;
use crate::command_center::{
    Cancel, Hit, HitSink, Query, Request, ResultSource, Results, Row, Scope, Selection,
};
use crate::prelude::*;

const LIMIT: usize = 500;

pub(super) fn recent() -> Vec<Row> {
    Results::recent(Vec::new(), Vec::new())
}

pub(super) struct Search {
    sources: Arc<[Arc<dyn ResultSource>]>,
    generation: u64,
    cancel: Cancel,
    sink: HitSink,
}

impl Search {
    pub(super) fn new(sources: Vec<Arc<dyn ResultSource>>) -> Self {
        Self {
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
            let rows = if open.step.is_some() {
                Vec::new()
            } else {
                recent()
            };
            return self.show_rows((generation, rows), window, cx);
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
