use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_core::marks::{Bookmarks, MarkSet, SearchHistory};
use rusty_core::paging::Needle;

use super::find::numbered_lines;
use super::search::{editor_view, paged_marking, search_for};
use super::{Log, Setup, hooks, press_only, redraw, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle, type_in, viewer_open};
use crate::window_view::viewer::{FileHooks, FileText, Kinds};
use rusty_core::encoding::TextEncoding;

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

fn colour_and_bookmarks(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
) -> (Option<u8>, usize) {
    paged_marking(cx, view, |marking| {
        (
            marking.marks.set.colour_of(&needle()),
            marking.bookmarks.iter().count(),
        )
    })
}

#[gpui_kit::test]
fn a_kind_switch_hands_marks_and_bookmarks_to_the_next_view(cx: &mut TestAppContext) {
    let recorder = Recorder::default();
    let files = recorder.hooks(Setup::new(numbered_lines()));
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    search_for(cx, window, &view, "needle");
    press(cx, window, &view, "ctrl-3");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "ctrl-f2");
    press(cx, window, &view, "alt-3");
    redraw(cx, window);
    assert_eq!(colour_and_bookmarks(cx, &view), (Some(2), 1));
    let loads = recorder
        .calls()
        .iter()
        .filter(|call| **call == "memory")
        .count();
    assert_eq!(loads, 1);
}

#[gpui_kit::test]
fn editor_marks_stay_in_the_session_when_its_text_is_not_the_file_bytes(cx: &mut TestAppContext) {
    let recorder = Recorder::default();
    let bom = TextEncoding {
        bom: true,
        ..TextEncoding::default()
    };
    let text = b"one needle\ntwo\n".to_vec();
    let read = recorder.hooks(Setup::new(text.clone())).read;
    let files = FileHooks {
        read: Arc::new(move |location| {
            Ok(FileText {
                encoding: bom,
                ..read(location)?
            })
        }),
        ..recorder.hooks(Setup::new(text))
    };
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    search_for(cx, window, &view, "needle");
    press(cx, window, &view, "ctrl-3");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "ctrl-f2");
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &view);
    let editor = editor_view(cx, &view);
    let marked = editor.read_with(cx, |editor, _| {
        let marking = editor.marking_state();
        (
            marking.marks.set.colour_of(&needle()),
            marking.bookmarks.iter().count(),
        )
    });
    assert_eq!(marked, (Some(2), 1));
    let calls = recorder.calls();
    assert!(
        !calls.contains(&"memory") && !calls.contains(&"save"),
        "{calls:?}"
    );
}

fn bookmarks(starts: &[u64]) -> Bookmarks {
    let mut bookmarks = Bookmarks::default();
    for at in starts {
        bookmarks.toggle(*at);
    }
    bookmarks
}

fn paged_bookmarks(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<u64> {
    paged_marking(cx, view, |marking| marking.bookmarks.iter().collect())
}

fn editor_bookmarks(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<u64> {
    let editor = editor_view(cx, view);
    editor.read_with(cx, |editor, _| {
        editor.marking_state().bookmarks.iter().collect()
    })
}

fn stored_bookmarks(recorder: &Recorder) -> Vec<u64> {
    let stored = recorder.stored.lock().expect("log").clone();
    stored.map_or_else(Vec::new, |(_, bookmarks)| bookmarks.iter().collect())
}

fn read_as(recorder: &Recorder, setup: impl Fn() -> Setup, file: FileText) -> FileHooks {
    let stamp = recorder.hooks(setup()).read;
    FileHooks {
        read: Arc::new(move |location| {
            Ok(FileText {
                stamp: stamp(location)?.stamp,
                ..file.clone()
            })
        }),
        ..recorder.hooks(setup())
    }
}

#[gpui_kit::test]
fn bookmarks_survive_a_trip_through_an_editor_that_cannot_keep_them(cx: &mut TestAppContext) {
    let recorder = Recorder::default();
    *recorder.stored.lock().expect("log") = Some((MarkSet::default(), bookmarks(&[16])));
    let text = b"one needle\ntwo\nthree\nfour\n".to_vec();
    let bom = FileText {
        encoding: TextEncoding {
            bom: true,
            ..TextEncoding::default()
        },
        ..FileText::plain(String::from_utf8_lossy(&text).into_owned())
    };
    let files = read_as(&recorder, || Setup::new(text.clone()), bom);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    press(cx, window, &view, "alt-3");
    settle(cx, &view);
    press(cx, window, &view, "alt-1");
    press(cx, window, &view, "alt-3");
    settle(cx, &view);
    assert_eq!(paged_bookmarks(cx, &view), [16]);
    press(cx, window, &view, "ctrl-f2");
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &view);
    assert_eq!(stored_bookmarks(&recorder), [0, 16]);
}

#[gpui_kit::test]
fn crlf_editor_bookmarks_are_stored_and_reloaded_at_file_offsets(cx: &mut TestAppContext) {
    let recorder = Recorder::default();
    let disk = b"one\r\ntwo\r\nthree\r\n".to_vec();
    let file = FileText {
        crlf: true,
        ..FileText::plain("one\ntwo\nthree\n")
    };
    let files = read_as(&recorder, || Setup::new(disk.clone()), file);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    let editor = super::editor_of(cx, &view).expect("the text editor is open");
    editor.update(cx, |state, cx| state.set_selected_range(8..8, cx));
    press(cx, window, &view, "ctrl-f2");
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &view);
    assert_eq!(stored_bookmarks(&recorder), [10]);
    press(cx, window, &view, "alt-3");
    settle(cx, &view);
    assert_eq!(paged_bookmarks(cx, &view), [10]);
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
    press(cx, window, &view, "f3");
    settle(cx, &view);
    assert_eq!(editor_bookmarks(cx, &view), [8]);
}

#[gpui_kit::test]
fn bookmarks_in_an_edited_mixed_file_reopen_on_the_same_text(cx: &mut TestAppContext) {
    let recorder = Recorder::default();
    let disk = b"one\r\ntwo\nthree\r\nfour\n".to_vec();
    let (unified, ends) =
        rusty_core::line_ends::LineEnds::unify("one\r\ntwo\nthree\r\nfour\n").expect("mixed");
    let file = FileText {
        ends: Some(Arc::new(ends)),
        ..FileText::plain(unified)
    };
    let files = read_as(&recorder, || Setup::new(disk.clone()), file);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    let editor = super::editor_of(cx, &view).expect("the text editor is open");
    editor.update(cx, |state, cx| state.set_selected_range(8..8, cx));
    press(cx, window, &view, "ctrl-f2");
    press(cx, window, &view, "ctrl-e");
    editor.update(cx, |state, cx| state.set_selected_range(0..0, cx));
    type_in(cx, window, &view, "new\n");
    press(cx, window, &view, "escape");
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx, &view);
    assert_eq!(stored_bookmarks(&recorder), [9]);
    press(cx, window, &view, "f3");
    settle(cx, &view);
    assert_eq!(editor_bookmarks(cx, &view), [8]);
}

#[gpui_kit::test]
fn a_second_kind_switch_during_a_load_keeps_the_handed_marks(cx: &mut TestAppContext) {
    let (window, view, _) = super::text_window(cx, Setup::new(b"one needle\ntwo\n".to_vec()));
    search_for(cx, window, &view, "needle");
    press(cx, window, &view, "ctrl-3");
    press(cx, window, &view, "escape");
    press_only(cx, window, "alt-3");
    press_only(cx, window, "alt-1");
    settle(cx, &view);
    let editor = editor_view(cx, &view);
    let colour = editor.read_with(cx, |editor, _| {
        editor.marking_state().marks.set.colour_of(&needle())
    });
    assert_eq!(colour, Some(2));
}
