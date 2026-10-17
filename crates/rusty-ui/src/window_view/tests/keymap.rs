use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Modifiers, TestAppContext};

use super::fkeys::{hold, key_labels};
use super::{FerriteWindow, StartOptions, fake, open_options, options_for, press, two_panes};
use crate::commands::Keymap;
use crate::window_view::{KeymapHooks, KeymapNews, KeymapSender};

const WARNING: &str = "keymap.toml, line 1: `ctrl-f3` now runs `files.sort_by_size`";
const ERROR: &str = "keymap.toml, line 2: unknown command `files.teleport`";

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    sender: KeymapSender,
}

fn rig(cx: &mut TestAppContext, start: Option<KeymapNews>) -> Rig {
    let fs = fake();
    fs.add_file("home/b.txt", 9);
    fs.add_file("home/c.txt", 1);
    let state = two_panes(&fs);
    let (keymap, sender) = KeymapHooks::channel(start);
    let options = StartOptions {
        keymap,
        ..options_for(fs, state)
    };
    let (window, view) = open_options(cx, options);
    Rig {
        window,
        view,
        sender,
    }
}

fn user(text: &str, warnings: &[&str]) -> KeymapNews {
    let overlaid = Keymap::builtin().overlay(text).expect("valid user keymap");
    KeymapNews::Applied {
        keymap: overlaid.keymap,
        warnings: warnings.iter().map(|&warning| warning.to_owned()).collect(),
    }
}

fn size_on_ctrl_f3() -> KeymapNews {
    user(
        "[[bind]]\ncommand = \"files.sort_by_size\"\nkeys = [\"ctrl-f3\"]\n",
        &[WARNING],
    )
}

impl Rig {
    fn send(&self, cx: &mut TestAppContext, news: KeymapNews) {
        assert!(self.sender.send(news));
        cx.run_until_parked();
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    fn files(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            let pane = state.pane(state.focused_pane()).expect("pane");
            pane.active()
                .listing()
                .map(|listing| {
                    listing
                        .entries()
                        .iter()
                        .map(|entry| entry.name.clone())
                        .filter(|name| name.ends_with(".txt"))
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    fn status(&self, cx: &mut TestAppContext) -> String {
        let pane = self
            .view
            .read_with(cx, |view, _| view.browser.state().focused_pane());
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let status = window.find(("status", pane.get()));
            status.label().unwrap_or_default().to_owned()
        })
        .expect("window")
    }

    fn ctrl_labels(&self, cx: &mut TestAppContext) -> Vec<String> {
        let ctrl = Modifiers {
            control: true,
            ..Modifiers::default()
        };
        hold(cx, self.window, ctrl);
        let labels = key_labels(cx, self.window).0;
        hold(cx, self.window, Modifiers::default());
        labels
    }
}

fn sorted_by_size(files: &[String]) -> bool {
    files.get(1).is_some_and(|middle| middle == "a.txt")
}

#[gpui_kit::test]
fn a_new_user_keymap_changes_what_a_key_does_and_its_label(cx: &mut TestAppContext) {
    let rig = rig(cx, None);
    assert_eq!(rig.files(cx), ["a.txt", "b.txt", "c.txt"]);
    assert_eq!(rig.ctrl_labels(cx)[2], "F3 By name");
    rig.send(cx, size_on_ctrl_f3());
    rig.press(cx, "ctrl-f3");
    assert!(sorted_by_size(&rig.files(cx)), "{:?}", rig.files(cx));
    assert_eq!(rig.ctrl_labels(cx)[2], "F3 By size");
    let status = rig.status(cx);
    assert!(status.starts_with(WARNING), "{status}");
}

#[gpui_kit::test]
fn an_invalid_change_keeps_the_last_keymap_and_shows_the_error(cx: &mut TestAppContext) {
    let rig = rig(cx, None);
    rig.send(cx, size_on_ctrl_f3());
    rig.send(cx, KeymapNews::Invalid(ERROR.to_owned()));
    let status = rig.status(cx);
    assert!(status.starts_with(ERROR), "{status}");
    assert!(!status.contains(WARNING), "{status}");
    assert_eq!(rig.ctrl_labels(cx)[2], "F3 By size");
    rig.press(cx, "ctrl-f3");
    assert!(sorted_by_size(&rig.files(cx)), "{:?}", rig.files(cx));
    rig.send(cx, user("", &[]));
    let status = rig.status(cx);
    assert!(!status.contains(ERROR), "{status}");
    assert_eq!(rig.ctrl_labels(cx)[2], "F3 By name");
}

#[gpui_kit::test]
fn a_keymap_error_at_start_shows_and_the_built_in_keys_run(cx: &mut TestAppContext) {
    let rig = rig(cx, Some(KeymapNews::Invalid(ERROR.to_owned())));
    let status = rig.status(cx);
    assert!(status.starts_with(ERROR), "{status}");
    assert_eq!(rig.ctrl_labels(cx)[2], "F3 By name");
}

#[gpui_kit::test]
fn a_user_keymap_at_start_is_bound_before_the_first_key(cx: &mut TestAppContext) {
    let rig = rig(cx, Some(size_on_ctrl_f3()));
    rig.press(cx, "ctrl-f3");
    assert!(sorted_by_size(&rig.files(cx)), "{:?}", rig.files(cx));
}

#[gpui_kit::test]
fn the_delete_for_good_warning_follows_its_key(cx: &mut TestAppContext) {
    let rig = rig(cx, None);
    let moved = "[[bind]]\ncommand = \"files.delete\"\nkeys = [\"shift-f7\"]\n";
    rig.send(cx, user(moved, &[]));
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    hold(cx, rig.window, shift);
    let labels = key_labels(cx, rig.window).0;
    assert_eq!(labels[6], "F7 Delete for good");
    assert_ne!(labels[7], "F8 Delete for good");
}
