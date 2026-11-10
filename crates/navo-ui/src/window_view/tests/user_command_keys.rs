use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_core::ProviderPath;
use navo_fs::{Attributes, EntryKind, FakeFs, FsEntry};

use super::{NavoWindow, open_options, options_for, press, two_panes};
use crate::commands::{Keymap, for_action};
use crate::host_os::HOST_OS;
use crate::settings::{Change, Field, Values};
use crate::window_view::{
    EditorHooks, KeymapHooks, KeymapNews, KeymapSender, OpenHooks, SettingsHooks, StartOptions,
    UserCommandHooks, UserCommandInfo, UserCommandNews, UserCommandSender, UserInvocation,
};

const RUN: &str = "navo_window::RunUserCommand";
type Runs = Arc<Mutex<Vec<(String, UserInvocation)>>>;
type Opened = Arc<Mutex<Vec<(String, String)>>>;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    fs: Arc<FakeFs>,
    commands: UserCommandSender,
    keymap: KeymapSender,
    runs: Runs,
    template: Arc<Mutex<usize>>,
    opened: Opened,
}

fn home() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file("home/docs/x.txt", 1);
    let parent = FsEntry {
        name: ProviderPath::from_text(".."),
        kind: EntryKind::Folder,
        size: 0,
        modified_ms: None,
        attributes: Attributes::default(),
    };
    fs.add_entry("home/docs", parent);
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs.add_file(&format!("home/{name}"), 1);
    }
    Arc::new(fs)
}

fn rig_with(cx: &mut TestAppContext, template: Result<(), String>) -> Rig {
    let fs = home();
    let state = two_panes(&fs);
    let runs = Runs::default();
    let seen = runs.clone();
    let run = Arc::new(move |name: &str, invocation: UserInvocation| {
        seen.lock().unwrap().push((name.to_owned(), invocation));
    });
    let file = fs.location("home/user-commands.toml");
    let template_calls = Arc::new(Mutex::new(0));
    let counted = template_calls.clone();
    let (hooks, commands) = UserCommandHooks::channel(run, Some(file));
    let hooks = hooks.with_edit(Arc::new(move || {
        *counted.lock().unwrap() += 1;
        template.clone()
    }));
    let (keymap_hooks, keymap) = KeymapHooks::channel(None);
    let opened = Opened::default();
    let log = opened.clone();
    let options = StartOptions {
        keymap: keymap_hooks,
        editor: EditorHooks {
            open: Arc::new(move |program, file| {
                log.lock()
                    .unwrap()
                    .push((program.to_owned(), file.as_str().to_owned()));
                Ok(())
            }),
            ..EditorHooks::none()
        },
        settings: SettingsHooks {
            values: Values::default().with(&Change::text(Field::EditorPath, "code.exe".to_owned())),
            ..SettingsHooks::none()
        },
        open: OpenHooks {
            user_commands: Some(hooks),
            ..OpenHooks::none()
        },
        ..options_for(fs.clone(), state)
    };
    let (window, view) = open_options(cx, options);
    Rig {
        window,
        view,
        fs,
        commands,
        keymap,
        runs,
        template: template_calls,
        opened,
    }
}

fn rig(cx: &mut TestAppContext) -> Rig {
    rig_with(cx, Ok(()))
}

fn info(name: &str, key: &str) -> UserCommandInfo {
    UserCommandInfo {
        name: name.to_owned(),
        key: Some(key.to_owned()),
    }
}

impl Rig {
    fn applied(&self, cx: &mut TestAppContext, list: Vec<UserCommandInfo>) {
        let news = UserCommandNews::Applied(list, Vec::new());
        assert!(self.commands.send(news));
        cx.run_until_parked();
    }

    fn keys(&self, cx: &mut TestAppContext, keys: &[&str]) {
        for key in keys {
            press(cx, self.window, &self.view, key);
        }
    }

    fn runs(&self) -> Vec<(String, UserInvocation)> {
        self.runs.lock().unwrap().clone()
    }

    fn notice(&self, cx: &mut TestAppContext) -> Option<(String, bool)> {
        self.view.read_with(cx, |view, _| {
            let shown = view.status_notice()?;
            Some((shown.text.to_string(), shown.is_error))
        })
    }

    fn reload_keymap(&self, cx: &mut TestAppContext, text: &str) {
        let overlaid = Keymap::builtin().overlay(text).expect("valid keymap");
        let news = KeymapNews::Applied {
            keymap: overlaid.keymap,
            warnings: Vec::new(),
        };
        assert!(self.keymap.send(news));
        cx.run_until_parked();
    }

    fn run_bindings(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.update(|cx| {
            let keymap = cx.key_bindings();
            let keymap = keymap.borrow();
            keymap
                .bindings()
                .filter(|binding| binding.action().name() == RUN)
                .map(|binding| {
                    let keys: Vec<String> = binding
                        .keystrokes()
                        .iter()
                        .map(|key| key.inner().unparse(HOST_OS))
                        .collect();
                    keys.join(" ")
                })
                .collect()
        })
    }

    fn dispatch_edit(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.dispatch_action(Box::new(crate::window_view::keys::EditUserCommands), cx);
        })
        .expect("window");
        super::settle(cx, &self.view);
    }
}

#[test]
fn run_is_a_registered_data_action_and_edit_a_command() {
    let run = for_action(RUN).expect("run spec");
    assert_eq!(run.name, "user_commands.run");
    let entries = crate::commands::entries();
    let shown = |name: &str| entries.iter().find(|entry| entry.spec.name == name);
    assert!(!shown("user_commands.run").expect("run entry").in_palette);
    let edit = shown("user_commands.edit").expect("edit entry");
    assert!(edit.in_palette);
    assert_eq!(edit.spec.action, Some("navo_window::EditUserCommands"));
}

#[gpui_kit::test]
fn a_user_key_runs_the_command_with_the_cursor_and_both_folders(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.applied(cx, vec![info("Build", "ctrl-alt-b")]);
    rig.keys(cx, &["down", "ctrl-alt-b"]);
    let runs = rig.runs();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let (name, invocation) = &runs[0];
    assert_eq!(name, "Build");
    assert_eq!(
        invocation,
        &UserInvocation {
            file: Some(rig.fs.location("home/a.txt")),
            marked: Vec::new(),
            folder: Some(rig.fs.location("home")),
            other_folder: Some(rig.fs.location("home")),
            other_file: Some(rig.fs.location("home/docs")),
        }
    );
}

#[gpui_kit::test]
fn the_marked_items_come_in_row_order(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.applied(cx, vec![info("Build", "ctrl-alt-b")]);
    rig.keys(
        cx,
        &["down", "space", "down", "space", "down", "ctrl-alt-b"],
    );
    let runs = rig.runs();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let invocation = &runs[0].1;
    assert_eq!(invocation.file, Some(rig.fs.location("home/c.txt")));
    assert_eq!(
        invocation.marked,
        [rig.fs.location("home/a.txt"), rig.fs.location("home/b.txt")]
    );
}

#[gpui_kit::test]
fn the_cursor_on_the_parent_row_is_refused_and_nothing_runs(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.applied(cx, vec![info("Build", "ctrl-alt-b")]);
    rig.keys(cx, &["enter", "home", "ctrl-alt-b"]);
    assert_eq!(rig.runs(), []);
    let (text, is_error) = rig.notice(cx).expect("a refusal notice");
    assert!(is_error);
    assert!(text.contains("Build") && text.contains(".."), "{text}");
}

#[gpui_kit::test]
fn each_key_is_bound_once_and_stays_once_after_a_keymap_reload(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.applied(
        cx,
        vec![info("Build", "ctrl-alt-b"), info("Lint", "ctrl-alt-l")],
    );
    let mut before = rig.run_bindings(cx);
    before.sort();
    assert_eq!(before, ["ctrl-alt-b", "ctrl-alt-l"]);
    let text = "[[bind]]\ncommand = \"files.sort_by_size\"\nkeys = [\"ctrl-f3\"]\n";
    rig.reload_keymap(cx, text);
    rig.reload_keymap(cx, "");
    let mut after = rig.run_bindings(cx);
    after.sort();
    assert_eq!(after, ["ctrl-alt-b", "ctrl-alt-l"]);
    rig.keys(cx, &["down", "ctrl-alt-l"]);
    let names: Vec<String> = rig.runs().into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["Lint"]);
}

#[gpui_kit::test]
fn a_reload_that_removes_a_command_removes_its_key(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.applied(
        cx,
        vec![info("Build", "ctrl-alt-b"), info("Lint", "ctrl-alt-l")],
    );
    rig.applied(cx, vec![info("Lint", "ctrl-alt-l")]);
    assert_eq!(rig.run_bindings(cx), ["ctrl-alt-l"]);
    rig.keys(cx, &["down", "ctrl-alt-b"]);
    assert_eq!(rig.runs(), []);
}

#[gpui_kit::test]
fn a_reload_that_moves_a_key_runs_only_the_new_command(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.applied(
        cx,
        vec![info("Build", "ctrl-alt-b"), info("Lint", "ctrl-alt-l")],
    );
    rig.applied(
        cx,
        vec![info("Build", "ctrl-alt-l"), info("Lint", "ctrl-alt-b")],
    );
    rig.keys(cx, &["down", "ctrl-alt-b"]);
    rig.keys(cx, &["ctrl-alt-l"]);
    let names: Vec<String> = rig.runs().into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["Lint", "Build"]);
}

#[gpui_kit::test]
fn edit_writes_the_template_then_opens_the_file_in_the_editor(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.dispatch_edit(cx);
    assert_eq!(*rig.template.lock().unwrap(), 1);
    let opened = rig.opened.lock().unwrap().clone();
    assert_eq!(opened.len(), 1, "{opened:?}");
    assert_eq!(opened[0].0, "code.exe");
    assert!(opened[0].1.ends_with("user-commands.toml"), "{opened:?}");
}

#[gpui_kit::test]
fn edit_that_cannot_write_the_template_shows_why_and_opens_nothing(cx: &mut TestAppContext) {
    let rig = rig_with(cx, Err("the folder is read-only".to_owned()));
    rig.dispatch_edit(cx);
    assert_eq!(*rig.opened.lock().unwrap(), []);
    let (text, is_error) = rig.notice(cx).expect("a notice");
    assert!(is_error);
    assert!(text.contains("the folder is read-only"), "{text}");
}

#[gpui_kit::test]
fn a_user_key_that_a_file_list_command_uses_gives_a_warning_naming_it(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.applied(cx, vec![info("Squeeze", "ctrl-c")]);
    let (text, is_error) = rig.notice(cx).expect("a clash warning");
    assert!(!is_error);
    assert!(text.contains("Squeeze") && text.contains("Copy"), "{text}");
    rig.applied(cx, vec![info("Squeeze", "ctrl-alt-q")]);
    assert_eq!(rig.notice(cx), None);
}

#[gpui_kit::test]
fn a_refusal_goes_with_the_next_command_and_an_invalid_line_stays(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.applied(cx, vec![info("Build", "ctrl-alt-j")]);
    rig.keys(cx, &["enter", "home", "ctrl-alt-j"]);
    assert!(rig.notice(cx).is_some());
    rig.keys(cx, &["down"]);
    assert_eq!(rig.notice(cx), None);
    let line = "user-commands.toml, line 3: bad".to_owned();
    assert!(rig.commands.send(UserCommandNews::Invalid(line.clone())));
    cx.run_until_parked();
    rig.keys(cx, &["down"]);
    assert_eq!(rig.notice(cx), Some((line, true)));
}
