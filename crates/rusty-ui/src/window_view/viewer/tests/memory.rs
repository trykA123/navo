use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_core::marks::{Bookmarks, MarkSet, SearchHistory};
use rusty_core::paging::Needle;

use super::find::numbered_lines;
use super::search::{paged_marking, search_for};
use super::{Log, Setup, hooks, press_only, redraw, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle, viewer_open};
use crate::window_view::viewer::{FileHooks, Kinds};

type Stored = Arc<Mutex<Option<(MarkSet, Bookmarks)>>>;

#[derive(Clone, Default)]
pub(super) struct Recorder {
    pub stored: Stored,
    pub calls: Arc<Mutex<Vec<&'static str>>>,
}

impl Recorder {
    pub(super) fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().expect("log").clone()
    }

    fn call(&self, name: &'static str) {
        self.calls.lock().expect("log").push(name);
    }

    pub(super) fn hooks(&self, setup: Setup) -> FileHooks {
        let (loader, saver, history, remember) =
            (self.clone(), self.clone(), self.clone(), self.clone());
        FileHooks {
            memory: Arc::new(move |_, _| {
                loader.call("memory");
                loader.stored.lock().expect("log").clone()
            }),
            save_memory: Arc::new(move |_, _, marks, bookmarks| {
                saver.call("save");
                *saver.stored.lock().expect("log") = Some((marks.clone(), bookmarks.clone()));
            }),
            history: Arc::new(move || {
                history.call("history");
                SearchHistory::default()
            }),
            remember: Arc::new(move |_| remember.call("remember")),
            ..hooks(setup, &Log::default())
        }
    }
}

fn needle() -> Needle {
    Needle::Text {
        text: "needle".to_owned(),
        match_case: false,
    }
}

fn open_with(
    cx: &mut TestAppContext,
    recorder: &Recorder,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let files = recorder.hooks(Setup::huge(numbered_lines()));
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    (window, view)
}

#[gpui_kit::test]
fn marks_come_back_from_the_hooks_when_the_file_reopens(cx: &mut TestAppContext) {
    let recorder = Recorder::default();
    let (window, view) = open_with(cx, &recorder);
    press(cx, window, &view, "f3");
    search_for(cx, window, &view, "needle");
    press(cx, window, &view, "ctrl-3");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
    settle(cx, &view);
    let saved = recorder.stored.lock().expect("log").clone();
    assert_eq!(
        saved.and_then(|(marks, _)| marks.colour_of(&needle())),
        Some(2)
    );
    press(cx, window, &view, "f3");
    redraw(cx, window);
    let colour = paged_marking(cx, &view, |marking| marking.marks.set.colour_of(&needle()));
    assert_eq!(colour, Some(2));
    let ranges = paged_marking(cx, &view, |marking| {
        marking.marks.ranges_in(0..u64::MAX).len()
    });
    assert_eq!(ranges, 2);
}

#[gpui_kit::test]
fn the_memory_hooks_run_only_in_background_tasks(cx: &mut TestAppContext) {
    let recorder = Recorder::default();
    let (window, view) = open_with(cx, &recorder);
    press_only(cx, window, "f3");
    assert!(recorder.calls().is_empty());
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(recorder.calls(), ["history", "memory"]);
    press(cx, window, &view, "ctrl-f");
    super::search::type_only(cx, window, "needle");
    press_only(cx, window, "enter");
    press_only(cx, window, "ctrl-3");
    assert_eq!(recorder.calls(), ["history", "memory"]);
    settle(cx, &view);
    assert_eq!(recorder.calls(), ["history", "memory", "remember"]);
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &view);
    assert_eq!(recorder.calls(), ["history", "memory", "remember", "save"]);
}
