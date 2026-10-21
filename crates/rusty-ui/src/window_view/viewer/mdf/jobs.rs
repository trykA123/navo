use std::sync::Arc;
use std::sync::atomic::Ordering;

use rusty_automotive::parse_mdf;
use rusty_l10n::t;

use super::text::rows;
use super::{Index, Loaded, MdfView};
use crate::prelude::*;
use crate::window_view::viewer::background::{Job, spawn_job};
use crate::window_view::viewer::body::Host;

const READ_LIMIT: usize = 2 * 1024 * 1024;

pub(super) type Parsed = Result<Arc<Loaded>, String>;

pub(super) fn start_parse(host: &Host, cx: &mut Context<MdfView>) -> Job {
    let read = host.hooks.range.clone();
    let location = host.location.clone();
    spawn_job(
        cx,
        move |cancel, send| {
            let parsed = read(&location, 0, READ_LIMIT).map(|bytes| {
                let mdf = parse_mdf(&String::from_utf8_lossy(&bytes));
                Arc::new(Loaded {
                    rows: rows(&mdf),
                    mdf,
                })
            });
            if !cancel.load(Ordering::Relaxed) {
                send(parsed);
            }
        },
        MdfView::parsed,
    )
}

impl MdfView {
    fn parsed(&mut self, parsed: Parsed, cx: &mut Context<Self>) {
        self.index = match parsed {
            Ok(loaded) => Index::Ready(loaded),
            Err(error) => Index::Failed(t!("viewer-mdf-read-error", error = error).into()),
        };
        self.refresh_cursor();
        cx.notify();
    }
}
