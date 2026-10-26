use std::time::Duration;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{AsyncApp, Role, Task, WeakEntity};

use super::PagedView;
use super::lines::PagedKind;
use super::moves::Move;
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::files::SharedSource;

const POLL_EVERY: Duration = Duration::from_secs(1);
const DOT: f32 = 6.;
const TEXT_ONLY: &str = "Follow works in the text view. 1 switches to it.";

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
    start_line: Option<u64>,
    _poll: Option<Task<()>>,
}

fn probe_end(source: &SharedSource) {
    let _ = source.probe_end();
}

async fn poll_once(source: &SharedSource, this: &WeakEntity<PagedView>, cx: &mut AsyncApp) -> bool {
    cx.background_executor().timer(POLL_EVERY).await;
    let probe = source.clone();
    cx.background_spawn(async move { probe_end(&probe) }).await;
    this.update(cx, |_, cx| cx.notify()).is_ok()
}

impl PagedView {
    pub(super) fn toggle_follow(&mut self, cx: &mut Context<Self>) {
        if self.kind != PagedKind::Text {
            self.set_note(TEXT_ONLY, cx);
            return;
        }
        if self.follower.state == Follow::Off {
            self.follower = Follower {
                state: Follow::Following,
                start_line: None,
                _poll: Some(self.poll_task(cx)),
            };
            self.queue_move(Move::End, cx);
        } else {
            self.follower = Follower::default();
        }
        cx.notify();
    }

    pub(in crate::window_view::viewer) fn follow_from_line(
        &mut self,
        line: u64,
        cx: &mut Context<Self>,
    ) {
        if self.kind != PagedKind::Text {
            return;
        }
        self.follower = Follower {
            state: Follow::Following,
            start_line: Some(line),
            _poll: Some(self.poll_task(cx)),
        };
        if self.index_done {
            self.follow_indexed(cx);
        }
    }

    pub(super) fn follow_indexed(&mut self, cx: &mut Context<Self>) {
        if let Some(line) = self.follower.start_line.take() {
            self.queue_move(Move::Line(line), cx);
        }
    }

    fn poll_task(&self, cx: &mut Context<Self>) -> Task<()> {
        let source = self.source.clone();
        cx.spawn(async move |this, cx| while poll_once(&source, &this, cx).await {})
    }

    pub(super) fn follow_moved(&mut self, to: Move) {
        self.follower.state = match (self.follower.state, to) {
            (Follow::Paused, Move::End) => Follow::Following,
            (Follow::Following, Move::End | Move::Rows(1..)) => Follow::Following,
            (Follow::Following, _) => Follow::Paused,
            (state, _) => state,
        };
    }

    pub(super) fn follow_grew(&mut self, cx: &mut Context<Self>) {
        if self.follower.state == Follow::Following {
            self.queue_move(Move::End, cx);
        }
    }

    pub(super) fn is_following(&self) -> bool {
        self.follower.state != Follow::Off
    }

    pub(super) fn stop_following(&mut self) {
        self.follower = Follower::default();
    }

    pub(super) fn follow_label(&self) -> Option<&'static str> {
        match self.follower.state {
            Follow::Off => None,
            Follow::Following => Some("Following"),
            Follow::Paused => Some("Paused · Ctrl+End resumes"),
        }
    }

    pub(super) fn follow_badge(&self, t: &Theme) -> Option<impl IntoElement> {
        let label = self.follow_label()?;
        let is_live = self.follower.state == Follow::Following;
        let (text, fill, dot) = if is_live {
            (t.colors.accent_text, t.colors.selection, t.colors.accent)
        } else {
            (t.colors.muted, t.colors.rail, t.colors.faint)
        };
        Some(
            div()
                .id("paged-follow")
                .role(Role::Status)
                .aria_label(label)
                .test_support()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.spacing.xs)
                .px(t.spacing.sm)
                .py(t.spacing.xxs)
                .rounded(t.radii.sm)
                .border_1()
                .border_color(t.colors.line)
                .bg(fill)
                .text_size(t.typography.small)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(text)
                .child(div().size(px(DOT)).rounded_full().bg(dot))
                .child(label),
        )
    }
}
