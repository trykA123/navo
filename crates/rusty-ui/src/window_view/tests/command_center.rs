use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Role, TestAppContext};
use rusty_core::Location;

use super::{fake, open_options, options_for, press, settle, two_panes};
use crate::command_center::{
    ArgKind, Cancel, Group, Hit, HitSink, Request, ResultSource, Scope, Scopes, Target,
};
use crate::window_view::{FerriteWindow, Frecent};

struct Fixed(Vec<Hit>);

impl ResultSource for Fixed {
    fn id(&self) -> &'static str {
        "fixed"
    }

    fn group(&self) -> Group {
        Group::Folders
    }

    fn scopes(&self) -> Scopes {
        Scopes::ALL
    }

    fn search(&self, request: &Request, sink: &HitSink, _: &Cancel) {
        for hit in &self.0 {
            sink.push(request.generation, hit.clone());
        }
    }
}

fn hit(title: &str, group: Group, target: Target) -> Hit {
    Hit {
        source: "fixed",
        group,
        title: title.to_owned().into(),
        detail: "".into(),
        score: 1_000,
        indices: Vec::new(),
        target,
        shortcut: None,
    }
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
}

fn rig_with(cx: &mut TestAppContext, hits: Vec<Hit>) -> Rig {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    if !hits.is_empty() {
        view.update(cx, |view, _| {
            view.command_center.sources.push(Arc::new(Fixed(hits)));
        });
    }
    Rig { window, view }
}

fn rig(cx: &mut TestAppContext) -> Rig {
    rig_with(cx, Vec::new())
}

fn location(path: &str) -> Location {
    fake().location(path)
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    fn type_query(&self, cx: &mut TestAppContext, text: &str) {
        cx.update_window(self.window, |_, window, cx| window.input(text, cx))
            .expect("window");
        settle(cx, &self.view);
    }

    fn is_open(&self, cx: &mut TestAppContext) -> bool {
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        self.read(cx, |window| window.try_find("command-center").is_some())
    }

    fn read<R>(&self, cx: &mut TestAppContext, read: impl FnOnce(&mut gpui_kit::Window) -> R) -> R {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            read(window)
        })
        .expect("window")
    }

    fn rows(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.read(cx, |window| {
            let center = window.within("command-center");
            (0usize..)
                .map_while(|index| center.try_find(("command-center-row", index)))
                .map(|row| match (row.role(), row.label()) {
                    (Some(Role::Heading), Some(label)) => format!("# {label}"),
                    (_, label) => label.unwrap_or("-").to_owned(),
                })
                .collect()
        })
    }

    fn label(&self, cx: &mut TestAppContext, id: &'static str) -> Option<String> {
        self.read(cx, |window| {
            window
                .try_find(id)
                .and_then(|found| found.label().map(str::to_owned))
        })
    }

    fn selection(&self, cx: &mut TestAppContext) -> Option<usize> {
        self.view
            .read_with(cx, |view, _| view.command_center_selection())
    }

    fn pane_has_focus(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            view.update(cx, |view, cx| {
                let pane = view.browser.state().focused_pane();
                view.handle(pane, cx).is_focused(window)
            })
        })
        .expect("window")
    }

    fn here(&self, cx: &mut TestAppContext) -> String {
        self.view.read_with(cx, |view, _| {
            view.browser
                .state()
                .active_tab()
                .location()
                .as_str()
                .to_owned()
        })
    }

    fn tab_count(&self, cx: &mut TestAppContext) -> usize {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            state
                .pane(state.focused_pane())
                .map_or(0, |pane| pane.tabs().len())
        })
    }
}

#[gpui_kit::test]
fn ctrl_k_opens_and_closes_and_focus_returns_to_the_pane(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-k");
    assert!(rig.is_open(cx));
    assert!(!rig.pane_has_focus(cx));
    rig.press(cx, "ctrl-k");
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn an_empty_box_shows_only_the_prefix_hint(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-k");
    assert_eq!(rig.rows(cx).len(), 1);
}

#[gpui_kit::test]
fn an_empty_box_lists_the_most_frecent_folders_under_recent(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let (dest, file) = (location("dest"), location("home/a.txt"));
    rig.view.update(cx, |view, _| {
        view.command_center.hooks.frecent = Arc::new(move |_| {
            vec![
                Frecent {
                    location: dest.as_str().to_owned(),
                    is_folder: true,
                    weight: 9.0,
                },
                Frecent {
                    location: file.as_str().to_owned(),
                    is_folder: false,
                    weight: 8.0,
                },
            ]
        });
    });
    rig.press(cx, "ctrl-k");
    settle(cx, &rig.view);
    let rows = rig.rows(cx);
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(rows[0], "# Recent");
    assert!(rows[1].contains("dest"), "{rows:?}");
}

#[gpui_kit::test]
fn typing_finds_commands_of_the_focused_file_list(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "switch pane");
    let rows = rig.rows(cx);
    assert_eq!(rows.first().map(String::as_str), Some("# Top hit"));
    assert!(
        rows.iter().any(|row| row.starts_with("Switch pane")),
        "{rows:?}"
    );
}

#[gpui_kit::test]
fn arrows_and_tab_move_the_selection(cx: &mut TestAppContext) {
    let folder = Target::Open {
        location: location("dest"),
        folder: true,
    };
    let rig = rig_with(cx, vec![hit("refuge", Group::Folders, folder)]);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "ref");
    let first = rig.selection(cx).expect("a row is selected");
    rig.press(cx, "down");
    let second = rig.selection(cx).expect("a row is selected");
    assert!(second > first);
    rig.press(cx, "up");
    assert_eq!(rig.selection(cx), Some(first));
    rig.press(cx, "tab");
    assert!(rig.selection(cx) > Some(first));
    rig.press(cx, "shift-tab");
    assert_eq!(rig.selection(cx), Some(first));
}

#[gpui_kit::test]
fn enter_runs_the_action_on_the_pane_that_had_focus(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let before = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "switch pane");
    rig.press(cx, "enter");
    assert!(!rig.is_open(cx));
    let after = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_ne!(before, after);
}

#[gpui_kit::test]
fn alt_enter_opens_a_folder_in_a_new_tab(cx: &mut TestAppContext) {
    let folder = Target::Open {
        location: location("dest"),
        folder: true,
    };
    let rig = rig_with(cx, vec![hit("destination", Group::Folders, folder)]);
    let tabs = rig.tab_count(cx);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "/dest");
    rig.press(cx, "alt-enter");
    assert!(!rig.is_open(cx));
    assert_eq!(rig.tab_count(cx), tabs + 1);
    assert_eq!(rig.here(cx), location("dest").as_str());
}

#[gpui_kit::test]
fn enter_on_a_folder_navigates_the_active_pane(cx: &mut TestAppContext) {
    let folder = Target::Open {
        location: location("dest"),
        folder: true,
    };
    let rig = rig_with(cx, vec![hit("destination", Group::Folders, folder)]);
    let tabs = rig.tab_count(cx);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "/dest");
    rig.press(cx, "enter");
    assert_eq!(rig.tab_count(cx), tabs);
    assert_eq!(rig.here(cx), location("dest").as_str());
}

#[gpui_kit::test]
fn escape_clears_the_query_then_closes(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "switch");
    rig.press(cx, "escape");
    assert!(rig.is_open(cx));
    assert_eq!(rig.rows(cx).len(), 1);
    rig.press(cx, "escape");
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn results_of_an_older_generation_are_dropped(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "switch pane");
    let shown = rig.rows(cx);
    let generation = rig
        .view
        .read_with(cx, |view, _| view.command_center_generation())
        .expect("the box is open");
    let stale = hit(
        "Stale",
        Group::Commands,
        Target::Action("file_list::Refresh"),
    );
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |view, cx| {
            view.receive_hits((generation - 1, Scope::Mixed, vec![stale]), window, cx);
        });
    })
    .expect("window");
    assert_eq!(rig.rows(cx), shown);
}

#[gpui_kit::test]
fn an_argument_command_asks_for_its_value_then_runs(cx: &mut TestAppContext) {
    let go = Target::Arg("pane.navigate", ArgKind::Path);
    let rig = rig_with(cx, vec![hit("Go to folder…", Group::Commands, go)]);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "go to");
    rig.press(cx, "enter");
    assert!(rig.is_open(cx));
    assert_eq!(
        rig.label(cx, "command-center-chip").as_deref(),
        Some("Go to folder")
    );
    rig.type_query(cx, location("dest").as_str());
    rig.press(cx, "enter");
    assert!(!rig.is_open(cx));
    assert_eq!(rig.here(cx), location("dest").as_str());
}

#[gpui_kit::test]
fn escape_leaves_the_argument_step_before_closing(cx: &mut TestAppContext) {
    let go = Target::Arg("pane.navigate", ArgKind::Path);
    let rig = rig_with(cx, vec![hit("Go to folder…", Group::Commands, go)]);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "go to");
    rig.press(cx, "enter");
    rig.press(cx, "escape");
    assert!(rig.is_open(cx));
    assert!(rig.label(cx, "command-center-chip").is_none());
}

fn recording(cx: &mut TestAppContext, rig: &Rig) -> Arc<Mutex<Vec<String>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    rig.view.update(cx, |view, _| {
        view.command_center.hooks.record_command = Arc::new(move |name| {
            sink.lock().unwrap().push(name.to_owned());
        });
    });
    seen
}

#[gpui_kit::test]
fn running_a_command_from_the_box_records_its_use(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let seen = recording(cx, &rig);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "switch pane");
    rig.press(cx, "enter");
    cx.run_until_parked();
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["file_list::SwitchPane".to_owned()]
    );
}

#[gpui_kit::test]
fn closing_without_running_and_running_a_folder_record_nothing(cx: &mut TestAppContext) {
    let rig = rig_with(
        cx,
        vec![hit("dest", Group::Folders, Target::GoTo(location("dest")))],
    );
    let seen = recording(cx, &rig);
    rig.press(cx, "ctrl-k");
    rig.press(cx, "escape");
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "dest");
    rig.press(cx, "enter");
    cx.run_until_parked();
    assert!(seen.lock().unwrap().is_empty());
}
