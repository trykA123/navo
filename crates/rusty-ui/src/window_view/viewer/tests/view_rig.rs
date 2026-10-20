use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext};
use rusty_core::shell_menu::MenuItem;
use rusty_fs::FakeFs;

use super::{Log, Setup, hooks, options_with, redraw};
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::tests::{open_options, press, settle};
use crate::window_view::viewer::Kinds;
use crate::window_view::viewer::editor::{EditorBody, EditorView};
use crate::window_view::{FerriteWindow, SaveReport, SettingsHooks, SettingsSender};

pub(super) struct Rig {
    pub(super) window: AnyWindowHandle,
    pub(super) view: Entity<FerriteWindow>,
    pub(super) saved: Arc<Mutex<Vec<Change>>>,
    pub(super) failure: Arc<Mutex<Option<String>>>,
    pub(super) news: SettingsSender,
}

fn two_files(first: &str, second: &str, len: u64) -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file(&format!("home/{first}"), len);
    fs.add_file(&format!("home/{second}"), len);
    fs.add_folder("dest");
    Arc::new(fs)
}

pub(super) fn rig_on(
    cx: &mut TestAppContext,
    names: (&str, &str),
    setup: Setup,
    values: Values,
) -> Rig {
    let fs = two_files(names.0, names.1, setup.bytes.len() as u64);
    let files = hooks(setup, &Log::default());
    let saved: Arc<Mutex<Vec<Change>>> = Arc::default();
    let stored = Arc::new(Mutex::new(values.clone()));
    let log = saved.clone();
    let failure: Arc<Mutex<Option<String>>> = Arc::default();
    let failing = failure.clone();
    let (settings, news) = SettingsHooks {
        values,
        save: Arc::new(move |changes| {
            if let Some(error) = failing.lock().expect("failure").clone() {
                return SaveReport {
                    failed: changes.iter().map(|change| change.field).collect(),
                    error: Some(error),
                    ..SaveReport::saved(stored.lock().expect("stored").clone())
                };
            }
            log.lock().expect("saved").extend_from_slice(changes);
            let mut stored = stored.lock().expect("stored");
            *stored = changes
                .iter()
                .fold(stored.clone(), |all, one| all.with(one));
            SaveReport::saved(stored.clone())
        }),
        ..SettingsHooks::none()
    }
    .channel();
    let options = crate::window_view::StartOptions {
        settings,
        ..options_with(fs, files, Kinds::builtin())
    };
    let (window, view) = open_options(cx, options);
    let rig = Rig {
        window,
        view,
        saved,
        failure,
        news,
    };
    rig.press(cx, "down");
    rig.press(cx, "f3");
    rig.settle(cx);
    rig
}

pub(super) fn rig(cx: &mut TestAppContext, names: (&str, &str), bytes: &[u8]) -> Rig {
    rig_on(cx, names, Setup::new(bytes.to_vec()), Values::default())
}

impl Rig {
    pub(super) fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    pub(super) fn settle(&self, cx: &mut TestAppContext) {
        cx.executor().advance_clock(Duration::from_secs(1));
        settle(cx, &self.view);
        redraw(cx, self.window);
        settle(cx, &self.view);
        redraw(cx, self.window);
    }

    pub(super) fn menu_items(&self, cx: &mut TestAppContext) -> Option<Vec<MenuItem>> {
        self.view.read_with(cx, |view, cx| {
            let menu = view.viewer_menu_view()?;
            Some(menu.read(cx).menu().items.clone())
        })
    }

    pub(super) fn rows(&self, cx: &mut TestAppContext) -> Vec<(String, bool)> {
        self.menu_items(cx)
            .unwrap_or_default()
            .into_iter()
            .map(|item| (item.text, item.checked))
            .collect()
    }

    pub(super) fn is_checked(&self, cx: &mut TestAppContext, text: &str) -> bool {
        self.rows(cx)
            .into_iter()
            .any(|(row, checked)| row == text && checked)
    }

    pub(super) fn line_ends(&self, cx: &mut TestAppContext) -> bool {
        let editor = self.editor(cx);
        editor.read_with(cx, |editor, _| editor.marking_state().line_ends)
    }

    pub(super) fn line_numbers(&self, cx: &mut TestAppContext) -> bool {
        let editor = self.editor(cx);
        editor.read_with(cx, |editor, _| editor.shows_line_numbers())
    }

    pub(super) fn editor(&self, cx: &mut TestAppContext) -> Entity<EditorView> {
        self.view
            .read_with(cx, |view, _| {
                let body = view.viewer.as_ref()?.body.as_ref()?;
                let editor = body.as_any().downcast_ref::<EditorBody>()?;
                Some(editor.0.clone())
            })
            .expect("the text view is open")
    }

    pub(super) fn shown_file(&self, cx: &mut TestAppContext) -> String {
        self.view.read_with(cx, |view, _| {
            view.viewer_location()
                .map(|location| location.as_str().to_owned())
                .unwrap_or_default()
        })
    }

    pub(super) fn saved(&self) -> Vec<Change> {
        self.saved.lock().expect("saved").clone()
    }

    pub(super) fn setting(&self, cx: &mut TestAppContext, field: Field) -> Value {
        self.view
            .read_with(cx, |view, _| view.values.get(field).clone())
    }
}

pub(super) fn shows(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> bool {
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|found| found.visible())
    })
    .expect("window")
}
