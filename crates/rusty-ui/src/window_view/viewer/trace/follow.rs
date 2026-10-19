use std::io::BufReader;
use std::time::Duration;

use gpui_kit::{AsyncApp, Task, WeakEntity};
use rusty_automotive::{IndexError, Trace, Watch};

use super::TraceView;
use crate::prelude::*;
use crate::window_view::viewer::background::{Ranged, ranged, spawn_job};

const POLL_EVERY: Duration = Duration::from_secs(1);

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
                let source = BufReader::new(Ranged {
                    read,
                    at,
                    len: u64::MAX,
                });
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
            Ok(tail) if tail.end > self.trace.end => self.append(tail, cx),
            Ok(_) => {}
            Err(error) => self.note = Some(error.to_string().into()),
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
