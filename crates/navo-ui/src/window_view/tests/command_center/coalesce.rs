use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::super::{fake, open_options, options_for, press, settle, two_panes};
use super::{Rig, hit};
use crate::command_center::{Cancel, Group, HitSink, Request, ResultSource, Scopes, Target};

struct Growing {
    searches: AtomicUsize,
    more: AtomicUsize,
}

impl Default for Growing {
    fn default() -> Self {
        Self {
            searches: AtomicUsize::new(0),
            more: AtomicUsize::new(10),
        }
    }
}

impl Growing {
    fn searches(&self) -> usize {
        self.searches.load(Ordering::SeqCst)
    }
}

impl ResultSource for Growing {
    fn id(&self) -> &'static str {
        "growing"
    }

    fn group(&self) -> Group {
        Group::Files
    }

    fn scopes(&self) -> Scopes {
        Scopes::ALL
    }

    fn search(&self, request: &Request, sink: &HitSink, _: &Cancel) {
        let round = self.searches.fetch_add(1, Ordering::SeqCst);
        sink.push(
            request.generation,
            hit(
                &format!("grow-{round}"),
                Group::Files,
                Target::Action("file_list::Refresh"),
            ),
        );
    }

    fn wait_for_more(&self, _: &Request, _: &Cancel) -> bool {
        if self.more.load(Ordering::SeqCst) == 0 {
            return false;
        }
        self.more.fetch_sub(1, Ordering::SeqCst);
        true
    }
}

fn growing_rig(cx: &mut TestAppContext) -> (Rig, Arc<Growing>) {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    let source = Arc::new(Growing::default());
    view.update(cx, |view, _| {
        view.command_center.sources.push(source.clone());
    });
    (Rig { window, view }, source)
}

#[gpui_kit::test]
fn coalesces_walk_rounds_and_keeps_the_final_hits(cx: &mut TestAppContext) {
    let (rig, source) = growing_rig(cx);
    press(cx, rig.window, &rig.view, "ctrl-k");
    cx.update_window(rig.window, |_, window, cx| window.input("grow", cx))
        .expect("window");
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(100));
    settle(cx, &rig.view);

    assert_eq!(source.searches(), 1);
    for _ in 0..10 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        settle(cx, &rig.view);
    }
    assert_eq!(source.searches(), 11);
    let rows = rig.rows(cx);
    assert!(
        rows.iter().any(|row| row.contains("grow-10")),
        "the last round is missing from {rows:?}"
    );
}
