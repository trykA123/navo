use std::io::BufReader;
use std::time::Duration;

use gpui_kit::{AsyncApp, Task, WeakEntity};
use rusty_automotive::{IndexError, Trace, Watch};
use rusty_l10n::tr;

use super::TraceView;
use crate::prelude::*;
use crate::window_view::viewer::background::{Ranged, ranged, spawn_job};

const POLL_EVERY: Duration = Duration::from_secs(1);
pub(super) const PREFIX: usize = 64;

pub(super) struct Probe {
    len: u64,
    replaced: bool,
    prefix: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Follow {
    #[default]
    Off,
    Following,
    Paused,
}

#[derive(Default)]
pub(super) struct Follower {
    pub state: Follow,
    _poll: Option<Task<()>>,
}

async fn poll_once(this: &WeakEntity<TraceView>, cx: &mut AsyncApp) -> bool {
    cx.background_executor().timer(POLL_EVERY).await;
    this.update(cx, |view, cx| view.read_tail(cx)).is_ok()
}

impl TraceView {
    pub(super) fn toggle_follow(&mut self, cx: &mut Context<Self>) {
        if !self.trace.supports_follow() {
            self.set_note(tr("trace-follow-unavailable"), cx);
            return;
        }
        if self.follower.state == Follow::Off {
            let poll = cx.spawn(async move |this, cx| while poll_once(&this, cx).await {});
            self.follower = Follower {
                state: Follow::Following,
                _poll: Some(poll),
            };
            self.go_last(cx);
        } else {
            self.follower = Follower::default();
        }
        cx.notify();
    }

    fn read_tail(&mut self, cx: &mut Context<Self>) {
        if self.reading.is_some() || self.follower.state == Follow::Off {
            return;
        }
        let head = self.host.hooks.head.clone();
        let location = self.host.location.clone();
        let mut read = ranged(&self.host.hooks, &self.host.location);
        let (end, prefix) = (self.trace.end, self.prefix.clone());
        let job = spawn_job(
            cx,
            move |_, send| {
                let probe = head(&location).and_then(|head| {
                    let last = match end.checked_sub(1) {
                        Some(at) if head.len >= end => read(at, 1)?,
                        _ => b"\n".to_vec(),
                    };
                    let replaced = head.len < end
                        || last.first() != Some(&b'\n')
                        || !head.bytes.starts_with(&prefix);
                    Ok(Probe {
                        len: head.len,
                        replaced,
                        prefix: head.bytes.into_iter().take(PREFIX).collect(),
                    })
                });
                send(probe);
            },
            Self::probed,
        );
        self.reading = Some(job);
    }

    fn probed(&mut self, probe: Result<Probe, String>, cx: &mut Context<Self>) {
        self.reading = None;
        match probe {
            Err(error) => self.read_error = Some(error.into()),
            Ok(probe) if probe.replaced => self.reindex(probe, cx),
            Ok(probe) => {
                self.len = probe.len;
                if probe.len > self.trace.end {
                    self.read_grown(probe.len, cx);
                }
            }
        }
        cx.notify();
    }

    fn reindex(&mut self, probe: Probe, cx: &mut Context<Self>) {
        self.len = probe.len;
        self.prefix = probe.prefix;
        self.trace = Trace::default();
        self.filter = None;
        self.generation += 1;
        self.fetched = None;
        self.fetching = None;
        self.failed_rows = None;
        self.inspected = None;
        self.inspecting = None;
        self.failed_inspect = None;
        self.index_error = None;
        self.read_error = None;
        self.nav = super::nav::Nav::default();
        self.cursor = 0;
        self.is_indexed = false;
        self.scanned = 0;
        self.is_replaced = true;
        self.reading = Some(super::jobs::start_index(&self.host, (probe.len, false), cx));
    }

    fn read_grown(&mut self, len: u64, cx: &mut Context<Self>) {
        let tail = self.trace.resume();
        let read = ranged(&self.host.hooks, &self.host.location);
        let job = spawn_job(
            cx,
            move |cancel, send| {
                let mut tail = tail;
                let mut progress = |_| {};
                let watch = Watch {
                    cancel,
                    progress: &mut progress,
                };
                let at = tail.end;
                let source = BufReader::new(Ranged { read, at, len });
                let result = tail.read_more(source, watch, u64::MAX);
                send(result.map(|_| tail));
            },
            Self::tail_read,
        );
        self.reading = Some(job);
    }

    fn tail_read(&mut self, result: Result<Trace, IndexError>, cx: &mut Context<Self>) {
        self.reading = None;
        match result {
            Ok(tail) => self.append(tail, cx),
            Err(error) => self.read_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    pub(super) fn follow_grew(&mut self, cx: &mut Context<Self>) {
        if self.follower.state == Follow::Following {
            self.go_last(cx);
        }
    }

    fn go_last(&mut self, cx: &mut Context<Self>) {
        let last = self.shown_rows().saturating_sub(1);
        self.move_cursor(last, cx);
    }

    pub(super) fn follow_moved(&mut self, to_end: bool) {
        self.follower.state = match (self.follower.state, to_end) {
            (Follow::Off, _) => Follow::Off,
            (_, true) => Follow::Following,
            (_, false) => Follow::Paused,
        };
    }

    pub(super) fn follow_label(&self) -> Option<&'static str> {
        match self.follower.state {
            Follow::Off => None,
            Follow::Following => Some("following"),
            Follow::Paused => Some("paused · Ctrl+End resumes"),
        }
    }
}
