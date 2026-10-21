use std::io::BufReader;
use std::sync::Arc;
use std::time::Duration;

use rusty_automotive::map::{MapError, index_map};

use super::loaded::Loaded;
use super::nav;
use super::select::{Query, select};
use super::{Index, MapView};
use crate::prelude::*;
use crate::window_view::viewer::background::{Job, Ranged, Throttle, ranged, spawn_job};
use crate::window_view::viewer::body::Host;

const PROGRESS_EVERY: Duration = Duration::from_millis(100);
const CHUNK: usize = 256 * 1024;

pub(super) enum IndexMessage {
    Progress(u64),
    Parsed(Result<(Arc<Loaded>, Vec<u32>), MapError>),
}

pub(super) struct Selected {
    query: Query,
    rows: Vec<u32>,
}

pub(super) fn start_index(host: &Host, len: u64, cx: &mut Context<MapView>) -> Job {
    let read = ranged(&host.hooks, &host.location);
    spawn_job(
        cx,
        move |cancel, send| {
            let mut throttle = Throttle::new(PROGRESS_EVERY);
            let mut progress = |done: u64| {
                throttle.when_due(|| send(IndexMessage::Progress(done)));
            };
            let source = BufReader::with_capacity(CHUNK, Ranged { read, at: 0, len });
            let parsed = index_map(source, cancel, &mut progress).and_then(|model| {
                let loaded = Loaded::new(model);
                let rows = select(&loaded, &Query::default(), cancel).ok_or(MapError::Cancelled)?;
                Ok((Arc::new(loaded), rows))
            });
            send(IndexMessage::Parsed(parsed));
        },
        MapView::index_message,
    )
}

impl MapView {
    pub(super) fn index_message(&mut self, message: IndexMessage, cx: &mut Context<Self>) {
        match message {
            IndexMessage::Progress(done) => self.scanned = done,
            IndexMessage::Parsed(Ok((loaded, rows))) => {
                self.nav_items = nav::items(&loaded);
                self.index = Index::Ready(loaded);
                let is_default = self.query == Query::default();
                self.rows = Arc::new(if is_default { rows } else { Vec::new() });
                self.move_cursor(0, cx);
                if !is_default {
                    self.run_select(cx);
                }
                self.start_model(cx);
            }
            IndexMessage::Parsed(Err(error)) => {
                self.index = Index::Failed(error.to_string().into());
            }
        }
        cx.notify();
    }

    pub(super) fn run_select(&mut self, cx: &mut Context<Self>) {
        let Index::Ready(loaded) = &self.index else {
            return;
        };
        let loaded = loaded.clone();
        let query = self.query.clone();
        self.selecting = Some(spawn_job(
            cx,
            move |cancel, send| {
                if let Some(rows) = select(&loaded, &query, cancel) {
                    send(Selected { query, rows });
                }
            },
            Self::selected_rows,
        ));
        cx.notify();
    }

    fn selected_rows(&mut self, selected: Selected, cx: &mut Context<Self>) {
        if selected.query != self.query {
            return;
        }
        let kept = self.wanted_row.take().or_else(|| {
            let kept = self.selected()?;
            u32::try_from(kept).ok()
        });
        self.selecting = None;
        self.rows = Arc::new(selected.rows);
        match kept.and_then(|kept| self.rows.iter().position(|at| *at == kept)) {
            Some(row) => self.move_cursor(row, cx),
            None => {
                self.go_address = None;
                self.move_cursor(0, cx);
                self.scroll.scroll_to_top(0);
            }
        }
    }
}
