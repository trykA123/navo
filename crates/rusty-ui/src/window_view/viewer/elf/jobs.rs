use std::sync::Arc;
use std::time::Duration;

use rusty_automotive::{Elf, ElfReader, IndexError, Lines, Query, Source, Watch, select};

use super::{ElfView, Index, NavItem, Phase};
use crate::prelude::*;
use crate::window_view::viewer::background::{Job, Throttle, ranged, spawn_job};
use crate::window_view::viewer::body::Host;

const PROGRESS_EVERY: Duration = Duration::from_millis(100);

pub(super) enum IndexMessage {
    Progress(Phase),
    Parsed(Result<(Arc<Elf>, Vec<u32>), IndexError>),
    Lines(Result<Lines, IndexError>),
}

pub(super) struct Selected {
    query: Query,
    rows: Vec<u32>,
}

fn nav_items(elf: &Elf) -> Vec<NavItem> {
    let sections = (1..elf.sections.len()).map(|at| NavItem::Section(at as u32));
    let segments = elf
        .segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| segment.is_load())
        .map(|(at, _)| NavItem::Segment(at));
    std::iter::once(NavItem::All)
        .chain(sections)
        .chain(segments)
        .collect()
}

pub(super) fn start_index(host: &Host, len: u64, cx: &mut Context<ElfView>) -> Job {
    let read = ranged(&host.hooks, &host.location);
    spawn_job(
        cx,
        move |cancel, send| {
            let mut throttle = Throttle::new(PROGRESS_EVERY);
            let mut symbols = |done: u64| {
                throttle.when_due(|| send(IndexMessage::Progress(Phase::Symbols(done))));
            };
            let mut watch = Watch {
                cancel,
                progress: &mut symbols,
            };
            let reader = ElfReader::new(Source::new(read, len));
            let parsed = reader.parse(&mut watch).and_then(|elf| {
                let rows = select(&elf, &Query::default(), cancel).ok_or(IndexError::Cancelled)?;
                Ok((Arc::new(elf), rows))
            });
            let elf = parsed.as_ref().ok().map(|(elf, _)| elf.clone());
            send(IndexMessage::Parsed(parsed));
            let Some(elf) = elf else {
                return;
            };
            let mut throttle = Throttle::new(PROGRESS_EVERY);
            let mut lines = |done: u64| {
                throttle.when_due(|| send(IndexMessage::Progress(Phase::Lines(done))));
            };
            let mut watch = Watch {
                cancel,
                progress: &mut lines,
            };
            send(IndexMessage::Lines(reader.lines(&elf, &mut watch)));
        },
        ElfView::index_message,
    )
}

impl ElfView {
    fn index_message(&mut self, message: IndexMessage, cx: &mut Context<Self>) {
        match message {
            IndexMessage::Progress(phase) => self.phase = phase,
            IndexMessage::Parsed(Ok((elf, rows))) => {
                self.nav_items = nav_items(&elf);
                self.index = Index::Ready(elf);
                self.rows = Arc::new(rows);
                self.phase = Phase::Lines(0);
                self.move_cursor(0, cx);
            }
            IndexMessage::Parsed(Err(error)) => {
                self.index = Index::Failed;
                self.phase = Phase::Done;
                self.note = Some(error.to_string().into());
            }
            IndexMessage::Lines(result) => {
                self.phase = Phase::Done;
                match result {
                    Ok(lines) => self.lines = Some(lines),
                    Err(error) => self.note = Some(error.to_string().into()),
                }
            }
        }
        cx.notify();
    }

    pub(super) fn run_select(&mut self, cx: &mut Context<Self>) {
        let Index::Ready(elf) = &self.index else {
            return;
        };
        let elf = elf.clone();
        let query = self.query.clone();
        self.selecting = Some(spawn_job(
            cx,
            move |cancel, send| {
                if let Some(rows) = select(&elf, &query, cancel) {
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
        let kept = self.selected().map(|(at, _)| at as u32);
        self.selecting = None;
        self.rows = Arc::new(selected.rows);
        match kept.and_then(|kept| self.rows.iter().position(|at| *at == kept)) {
            Some(row) => self.move_cursor(row, cx),
            None => {
                self.move_cursor(0, cx);
                self.scroll.scroll_to_top(0);
            }
        }
    }
}
