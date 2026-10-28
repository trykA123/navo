use std::time::Duration;

use futures::FutureExt as _;
use gpui_kit::Task;
use rusty_fs::CancelToken;

use super::super::FerriteWindow;
use super::open::Way;
use crate::command_center::content_source::PreviewEvent;
use crate::command_center::{ContentQuery, Mode, Results, Row, Selection, Target};
use crate::prelude::*;

const DEBOUNCE: Duration = Duration::from_millis(300);
const POLL: Duration = Duration::from_millis(16);
const TIMEOUT: Duration = Duration::from_secs(2);

pub(super) struct Flight {
    cancel: CancelToken,
    _task: Task<()>,
}

impl Drop for Flight {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl FerriteWindow {
    pub(super) fn toggle_content_flag(
        &mut self,
        regex: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self
            .command_center
            .open
            .as_mut()
            .filter(|open| open.mode == Mode::Content)
        else {
            return;
        };
        if regex {
            open.regex = !open.regex;
        } else {
            open.case = !open.case;
        }
        let query = open.center.read(cx).query(cx);
        self.search_content(&query, window, cx);
    }

    pub(super) fn search_content(
        &mut self,
        input: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let base = self.browser.state().active_tab().base_or_location().clone();
        let source = self.command_center.content_source.clone();
        let Some(open) = self.command_center.open.as_mut() else {
            return;
        };
        open.content_flight = None;
        let generation = open.search.next();
        open.content = ContentQuery::parse(input, open.case, open.regex);
        open.rows = Default::default();
        open.rows_memory.set(Results::memory_use(&open.rows));
        open.selection = Selection::start(&open.rows);
        open.error = None;
        open.indexing = None;
        let query = open.content.find.clone();
        let base = open.scope.clone().unwrap_or(base);
        let cancel = CancelToken::new();
        let signal = cancel.clone();
        let job = ContentJob {
            source,
            base,
            query,
            signal,
            generation,
        };
        let task = cx.spawn_in(window, async move |this, cx| job.run(this, cx).await);
        open.content_flight = Some(Flight {
            cancel,
            _task: task,
        });
        self.push_command_center(window, cx);
    }

    fn content_rows(
        &mut self,
        (generation, rows, error): (u64, Vec<Row>, Option<String>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) =
            self.command_center.open.as_mut().filter(|open| {
                open.mode == Mode::Content && open.search.generation() == generation
            })
        else {
            return;
        };
        open.selection = open.kept_selection(generation, &rows);
        open.search.shown = generation;
        open.rows = rows.into();
        open.rows_memory.set(Results::memory_use(&open.rows));
        open.error = error.map(Into::into);
        self.push_command_center(window, cx);
    }

    pub(super) fn run_content(&mut self, way: Way, window: &mut Window, cx: &mut Context<Self>) {
        let base = self.browser.state().active_tab().base_or_location().clone();
        let Some(open) = self.command_center.open.as_ref() else {
            return;
        };
        if way == Way::OtherPane {
            let query = open.content.find.clone();
            let base = open.scope.clone().unwrap_or(base);
            self.close_command_center(window, cx);
            let pane = self.browser.state().focused_pane();
            self.browser.start_find(pane, query, base);
            self.focus_core_pane(window, cx);
            cx.notify();
            return;
        }
        if way != Way::Open {
            return;
        }
        let Some(Row::Hit(hit)) = open.selection.row(&open.rows) else {
            return;
        };
        let Target::Content {
            location,
            line,
            entry,
        } = hit.target.clone()
        else {
            return;
        };
        self.close_command_center(window, cx);
        self.view_location(location, window, cx);
        if let Some(entry) = entry {
            self.select_archive_entry(entry);
        } else {
            self.open_viewer_at_line(Some(line), None);
        }
    }
}

fn input_is_empty(query: &rusty_core::find::FindQuery) -> bool {
    query.text.is_empty() && query.tokens.is_empty()
}

struct ContentJob {
    source: crate::command_center::ContentSource,
    base: rusty_core::Location,
    query: rusty_core::find::FindQuery,
    signal: CancelToken,
    generation: u64,
}

impl ContentJob {
    async fn run(
        self,
        this: gpui_kit::WeakEntity<FerriteWindow>,
        cx: &mut gpui_kit::AsyncWindowContext,
    ) {
        cx.background_executor().timer(DEBOUNCE).await;
        if self.signal.is_cancelled() || input_is_empty(&self.query) {
            return;
        }
        let preview = self
            .source
            .start(self.base, self.query, self.signal.clone());
        let mut rows = Vec::new();
        let deadline = cx.background_executor().timer(TIMEOUT).fuse();
        futures::pin_mut!(deadline);
        loop {
            let tick = cx.background_executor().timer(POLL).fuse();
            futures::pin_mut!(tick);
            futures::select_biased! {
                () = deadline => return,
                () = tick => {}
            }
            let before = rows.len();
            let (done, error) = receive_preview(&preview, &mut rows);
            if !done && rows.len() == before {
                continue;
            }
            let _ = this.update_in(cx, |view, window, cx| {
                view.content_rows((self.generation, rows.clone(), error), window, cx);
            });
            if done {
                return;
            }
        }
    }
}

fn receive_preview(
    preview: &crate::command_center::content_source::Preview,
    rows: &mut Vec<Row>,
) -> (bool, Option<String>) {
    while let Ok(event) = preview.receive.try_recv() {
        match event {
            PreviewEvent::Row(row) => rows.push(*row),
            PreviewEvent::Done(note) => return (true, note),
        }
    }
    (false, None)
}
