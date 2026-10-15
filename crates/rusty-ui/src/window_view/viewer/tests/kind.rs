use std::any::Any;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{AnyView, FocusHandle, TestAppContext};
use rusty_core::Location;

use super::find::bottom_keys;
use super::{Log, Setup, has_editor, hooks, redraw, text_of, window_with};
use crate::prelude::*;
use crate::window_view::tests::{fake, press};
use crate::window_view::viewer::{
    Attach, BodyKeys, FileHooks, Kinds, Loaded, Slot, Sniff, ViewerBody, ViewerKind,
};

struct NotesView {
    text: SharedString,
    focus: FocusHandle,
}

impl Render for NotesView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("notes-body")
            .track_focus(&self.focus)
            .aria_label(self.text.clone())
            .test_support()
            .size_full()
            .child(self.text.clone())
    }
}

struct NotesBody(Entity<NotesView>);

impl ViewerBody for NotesBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus.clone()
    }

    fn keys(&self, _: &App) -> BodyKeys {
        BodyKeys::Normal(Vec::new())
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, _: SharedString, _: &mut App) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct Notes {
    id: &'static str,
    name: &'static str,
    score: u8,
}

const NOTES: Notes = Notes {
    id: "notes",
    name: "Notes",
    score: 99,
};

const MEMO: Notes = Notes {
    id: "memo",
    name: "Memo",
    score: 98,
};

impl ViewerKind for Notes {
    fn id(&self) -> &'static str {
        self.id
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if sniff.name.ends_with(".txt") {
            self.score
        } else {
            0
        }
    }

    fn load(
        &self,
        hooks: &FileHooks,
        location: &Location,
        _: &Sniff<'_>,
    ) -> Result<Loaded, String> {
        let file = (hooks.read)(location)?;
        Ok(Box::new(format!("{}: {}", self.id, file.text.trim_end())))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let text = attach
            .loaded
            .downcast::<String>()
            .map_err(|_| "Not notes.".to_owned())?;
        let view = cx.new(|cx| NotesView {
            text: (*text).into(),
            focus: cx.focus_handle(),
        });
        Ok(Box::new(NotesBody(view)))
    }
}

#[gpui_kit::test]
fn a_registered_kind_opens_first_and_slots_switch_to_and_from_it(cx: &mut TestAppContext) {
    let files = hooks(Setup::new(b"hello\n".to_vec()), &Log::default());
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin().with(NOTES));
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    assert_eq!(text_of(cx, window, "notes-body"), "notes: hello");
    let keys = bottom_keys(cx, window);
    assert!(keys.contains(&"1 Text".to_owned()), "{keys:?}");
    assert!(keys.contains(&"3 Hex".to_owned()), "{keys:?}");
    press(cx, window, &view, "1");
    assert!(has_editor(cx, &view));
    assert!(bottom_keys(cx, window).contains(&"5 Notes".to_owned()));
    press(cx, window, &view, "5");
    assert_eq!(text_of(cx, window, "notes-body"), "notes: hello");
}

#[gpui_kit::test]
fn the_slot_key_cycles_through_kinds_that_share_it(cx: &mut TestAppContext) {
    let files = hooks(Setup::new(b"hello\n".to_vec()), &Log::default());
    let kinds = Kinds::builtin().with(NOTES).with(MEMO);
    let (window, view) = window_with(cx, fake(), files, kinds);
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    assert_eq!(text_of(cx, window, "notes-body"), "notes: hello");
    assert!(bottom_keys(cx, window).contains(&"5 Memo".to_owned()));
    press(cx, window, &view, "5");
    assert_eq!(text_of(cx, window, "notes-body"), "memo: hello");
    assert!(bottom_keys(cx, window).contains(&"5 Notes".to_owned()));
    press(cx, window, &view, "5");
    assert_eq!(text_of(cx, window, "notes-body"), "notes: hello");
}
