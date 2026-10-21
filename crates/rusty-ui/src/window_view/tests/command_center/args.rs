use gpui_kit::TestAppContext;

use super::super::{open_options, options_for, settle, two_panes};
use super::late::{input_unsettled, keys_unsettled};
use super::{Rig, fake, rig};

fn rig_with_projects(cx: &mut TestAppContext) -> Rig {
    let fs = fake();
    fs.add_folder("dest/projects");
    fs.add_file("dest/readme.txt", 1);
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    Rig { window, view }
}

fn ask(cx: &mut TestAppContext, rig: &Rig, command: &str) {
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, command);
    rig.press(cx, "enter");
    assert!(rig.is_open(cx));
}

fn typed(rest: &str) -> String {
    format!("/dest/{rest}")
}

impl Rig {
    fn query(&self, cx: &mut TestAppContext) -> String {
        self.view
            .read_with(cx, |view, cx| view.command_center_query(cx))
            .expect("the box is open")
    }

    fn error(&self, cx: &mut TestAppContext) -> Option<String> {
        self.label(cx, "command-center-error")
    }

    fn marked(&self, cx: &mut TestAppContext) -> usize {
        self.view.read_with(cx, |view, _| {
            view.browser.state().active_tab().selection().marked().len()
        })
    }
}

#[gpui_kit::test]
fn go_to_folder_asks_for_a_path_then_navigates(cx: &mut TestAppContext) {
    let rig = rig_with_projects(cx);
    ask(cx, &rig, "go to folder");
    assert_eq!(
        rig.label(cx, "command-center-chip").as_deref(),
        Some("Go to folder")
    );
    rig.type_query(cx, &typed("projects"));
    rig.press(cx, "enter");
    cx.run_until_parked();
    assert!(!rig.is_open(cx));
    assert!(rig.here(cx).ends_with("/dest/projects"), "{}", rig.here(cx));
}

#[gpui_kit::test]
fn mark_by_pattern_asks_for_text_then_marks(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ask(cx, &rig, "mark by pattern…");
    assert_eq!(
        rig.label(cx, "command-center-chip").as_deref(),
        Some("Mark by pattern")
    );
    rig.type_query(cx, "*.txt");
    rig.press(cx, "enter");
    assert!(!rig.is_open(cx));
    assert_eq!(rig.marked(cx), 1);
}

#[gpui_kit::test]
fn a_bad_path_keeps_the_box_open_with_the_error(cx: &mut TestAppContext) {
    let rig = rig_with_projects(cx);
    let start = rig.here(cx);
    ask(cx, &rig, "go to folder");
    rig.type_query(cx, &typed("nowhere"));
    rig.press(cx, "enter");
    cx.run_until_parked();
    assert!(rig.is_open(cx));
    assert!(rig.error(cx).is_some_and(|text| text.contains("nowhere")));
    assert_eq!(rig.here(cx), start);
    rig.press(cx, "ctrl-a");
    rig.type_query(cx, &typed("readme.txt"));
    rig.press(cx, "enter");
    cx.run_until_parked();
    assert!(
        rig.error(cx)
            .is_some_and(|text| text.contains("not a folder"))
    );
    assert_eq!(rig.here(cx), start);
}

#[gpui_kit::test]
fn escape_leaves_the_step_with_the_query_then_closes(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ask(cx, &rig, "go to folder");
    rig.press(cx, "escape");
    assert!(rig.is_open(cx));
    assert!(rig.label(cx, "command-center-chip").is_none());
    assert_eq!(rig.query(cx), "go to folder");
    assert!(
        rig.rows(cx)
            .iter()
            .any(|row| row.starts_with("Go to folder"))
    );
    rig.press(cx, "escape");
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn tab_completes_a_folder_in_the_step(cx: &mut TestAppContext) {
    let rig = rig_with_projects(cx);
    ask(cx, &rig, "go to folder");
    rig.type_query(cx, &typed("pro"));
    rig.press(cx, "tab");
    settle(cx, &rig.view);
    assert_eq!(rig.query(cx), "/dest/projects/");
}

#[gpui_kit::test]
fn enter_waits_for_the_round_and_goes_to_the_listed_folder(cx: &mut TestAppContext) {
    let rig = rig_with_projects(cx);
    ask(cx, &rig, "go to folder");
    input_unsettled(cx, &rig, &typed("pro"));
    keys_unsettled(cx, &rig, &["enter"]);
    settle(cx, &rig.view);
    cx.run_until_parked();
    assert!(!rig.is_open(cx));
    assert!(rig.here(cx).ends_with("/dest/projects"), "{}", rig.here(cx));
}

#[gpui_kit::test]
fn open_folder_in_new_tab_asks_for_a_path_then_opens_a_tab(cx: &mut TestAppContext) {
    let rig = rig_with_projects(cx);
    let tabs = rig.tab_count(cx);
    ask(cx, &rig, "open folder in new tab");
    rig.type_query(cx, &typed("projects"));
    rig.press(cx, "enter");
    cx.run_until_parked();
    assert!(!rig.is_open(cx));
    assert_eq!(rig.tab_count(cx), tabs + 1);
    assert!(rig.here(cx).ends_with("/dest/projects"));
}
