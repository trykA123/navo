use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::base::Scrollbar;
use gpui_kit::test::{ScopedWindow, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, Entity, InteractiveElement, IntoElement,
    Modifiers, MouseButton, ParentElement, Render, Role, Styled, TestAppContext, Window, div,
    point, px, size,
};
use rusty_core::Location;

use crate::command_center::view::{
    Argument, CommandCenter, CommandCenterEvent, CommandCenterProps, Hint, HintKeys,
    highlight_ranges, hints, place, result_count,
};
use crate::command_center::{ArgKind, Group, Heading, Hit, Results, Row, Scope, Selection, Target};
use crate::commands::Keymap;
use crate::test_pointer::{dispatch, down, up};
use crate::test_window::open_sized;

const ROOT: &str = "command-center";

fn command(title: &str, shortcut: Option<&str>) -> Hit {
    Hit {
        source: "commands",
        group: Group::Commands,
        title: title.to_owned().into(),
        detail: "File list".into(),
        score: 10,
        indices: vec![0, 1, 2],
        target: Target::Action("file_list.copy"),
        shortcut: shortcut.map(|keys| keys.to_owned().into()),
    }
}

fn folder(title: &str, parent: &str, score: i64) -> Hit {
    Hit {
        source: "recent",
        group: Group::Folders,
        title: title.to_owned().into(),
        detail: parent.to_owned().into(),
        score,
        indices: Vec::new(),
        target: Target::Open {
            location: Location::new(parent).expect("a valid location"),
            folder: true,
        },
        shortcut: None,
    }
}

fn file(title: &str) -> Hit {
    Hit {
        group: Group::Files,
        target: Target::Open {
            location: Location::new("C:\\Docs").expect("a valid location"),
            folder: false,
        },
        ..folder(title, "C:\\Docs", 1)
    }
}

fn mixed_rows() -> Vec<Row> {
    let mut hits = vec![
        Hit {
            score: 99,
            ..command("Copy path", Some("ctrl-shift-c"))
        },
        command("Copy", Some("f5")),
        folder("copper-gauge", "C:\\Projects", 5),
    ];
    hits.extend((0..6).map(|n| Hit {
        score: 9 - n,
        ..command(&format!("Copy {n}"), None)
    }));
    Results::merge(hits, Scope::Mixed)
}

fn props(rows: Vec<Row>) -> CommandCenterProps {
    let selection = Selection::start(&rows);
    CommandCenterProps {
        mode: crate::command_center::Mode::Names,
        rows: rows.into(),
        selection,
        argument: None,
        note: None,
        error: None,
        anchor: Bounds::new(point(px(140.), px(8.)), size(px(520.), px(32.))),
        key_hints: Some(HintKeys::of(Keymap::builtin())),
        is_leaving: false,
    }
}

struct Harness {
    center: Entity<CommandCenter>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(div().id("outside").size_full())
            .child(self.center.clone())
    }
}

struct Opened {
    window: AnyWindowHandle,
    center: Entity<CommandCenter>,
    events: Rc<RefCell<Vec<CommandCenterEvent>>>,
}

fn open(cx: &mut TestAppContext, props: CommandCenterProps) -> Opened {
    let events: Rc<RefCell<Vec<CommandCenterEvent>>> = Rc::default();
    let sink = events.clone();
    let (window, harness) = open_sized(cx, px(1000.), move |window, cx| {
        let center = cx.new(|cx| CommandCenter::new(props, window, cx));
        cx.subscribe(&center, move |_, event: &CommandCenterEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
        .detach();
        Harness { center }
    });
    let center = harness.read_with(cx, |harness, _| harness.center.clone());
    let opened = Opened {
        window,
        center,
        events,
    };
    opened.read(cx, |_| ());
    opened
}

impl Opened {
    fn read<R>(&self, cx: &mut TestAppContext, read: impl FnOnce(&mut ScopedWindow<'_>) -> R) -> R {
        cx.run_until_parked();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            read(&mut window.within(ROOT))
        })
        .expect("window")
    }

    fn rows(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.read(cx, |window| {
            (0usize..)
                .map_while(|index| window.try_find(("command-center-row", index)))
                .map(|row| match (row.role(), row.label()) {
                    (Some(Role::Heading), Some(label)) => format!("# {label}"),
                    (_, label) => label.unwrap_or("-").to_owned(),
                })
                .collect()
        })
    }

    fn selected(&self, cx: &mut TestAppContext) -> Vec<usize> {
        self.read(cx, |window| {
            (0usize..)
                .map_while(|index| window.try_find(("command-center-row", index)))
                .enumerate()
                .filter(|(_, row)| row.selected() == Some(true))
                .map(|(index, _)| index)
                .collect()
        })
    }

    fn label(&self, id: &'static str, cx: &mut TestAppContext) -> Option<String> {
        self.read(cx, |window| {
            window
                .try_find(id)
                .and_then(|found| found.label().map(str::to_owned))
        })
    }

    fn events(&self) -> Vec<CommandCenterEvent> {
        self.events.borrow().clone()
    }

    fn update(&self, props: CommandCenterProps, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            self.center
                .update(cx, |center, cx| center.set_props(props, window, cx));
        })
        .expect("window");
    }
}

#[gpui_kit::test]
fn a_mixed_query_shows_the_top_hit_groups_and_show_all(cx: &mut TestAppContext) {
    let center = open(cx, props(mixed_rows()));
    assert_eq!(
        center.rows(cx),
        [
            "# Top hit",
            "Copy path",
            "# Commands",
            "Copy",
            "Copy 0",
            "Copy 1",
            "Copy 2",
            "Copy 3",
            "Show all 8",
            "# Folders",
            "copper-gauge",
        ]
    );
    assert_eq!(center.selected(cx), [1]);
}

#[gpui_kit::test]
fn a_command_row_shows_its_shortcut_as_key_caps(cx: &mut TestAppContext) {
    let center = open(cx, props(mixed_rows()));
    let key = center.read(cx, |window| {
        window
            .within(("command-center-row", 3usize))
            .try_find(("command-center-key", 0usize))
            .and_then(|key| key.label().map(str::to_owned))
    });
    assert_eq!(key.as_deref(), Some("F5"));
}

#[gpui_kit::test]
fn the_empty_state_lists_recent_rows_then_the_prefix_hint(cx: &mut TestAppContext) {
    let rows = Results::recent(
        vec![command("Go to folder", None)],
        vec![folder("Downloads", "C:\\Users\\admin", 1)],
    );
    let center = open(cx, props(rows));
    assert_eq!(
        center.rows(cx),
        [
            "# Recent",
            "Go to folder",
            "Downloads",
            "> commands · / files · C:\\ paths",
        ]
    );
}

#[gpui_kit::test]
fn no_match_names_the_query(cx: &mut TestAppContext) {
    let center = open(cx, props(Vec::new()));
    center.update(props(Vec::new()), cx);
    cx.update_window(center.window, |_, window, cx| {
        center
            .center
            .update(cx, |center, cx| center.set_query("qzxv", window, cx));
    })
    .expect("window");
    assert_eq!(
        center.label("command-center-empty", cx).as_deref(),
        Some("No commands, folders or files match qzxv.")
    );
}

#[gpui_kit::test]
fn the_selected_row_follows_the_props(cx: &mut TestAppContext) {
    let rows = mixed_rows();
    let mut next = props(rows.clone());
    next.selection.step(&rows, 2);
    let center = open(cx, props(rows));
    center.update(next, cx);
    assert_eq!(center.selected(cx), [4]);
}

#[gpui_kit::test]
fn typing_reports_the_query(cx: &mut TestAppContext) {
    let center = open(cx, props(Vec::new()));
    cx.update_window(center.window, |_, window, cx| window.input("cop", cx))
        .expect("window");
    cx.run_until_parked();
    assert_eq!(
        center.events().last(),
        Some(&CommandCenterEvent::QueryChanged("cop".into()))
    );
}

#[gpui_kit::test]
fn hover_moves_the_selection_and_never_runs(cx: &mut TestAppContext) {
    let center = open(cx, props(mixed_rows()));
    cx.update_window(center.window, |_, window, cx| {
        window
            .within(ROOT)
            .hover(("command-center-row", 4usize), cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(center.events(), [CommandCenterEvent::Moved(4)]);
}

#[gpui_kit::test]
fn a_click_runs_the_row(cx: &mut TestAppContext) {
    let center = open(cx, props(mixed_rows()));
    cx.update_window(center.window, |_, window, cx| {
        window
            .within(ROOT)
            .click(("command-center-row", 10usize), cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(
        center.events().last(),
        Some(&CommandCenterEvent::Run(10, Modifiers::default()))
    );
}

#[gpui_kit::test]
fn an_alt_click_runs_the_row_for_a_new_tab(cx: &mut TestAppContext) {
    let center = open(cx, props(mixed_rows()));
    let spot = center.read(cx, |window| {
        window
            .find(("command-center-row", 10usize))
            .bounds()
            .center()
    });
    let alt = Modifiers::alt();
    dispatch(cx, center.window, down(MouseButton::Left, spot, alt));
    dispatch(cx, center.window, up(MouseButton::Left, spot, alt));
    assert_eq!(
        center.events().last(),
        Some(&CommandCenterEvent::Run(10, alt))
    );
}

#[gpui_kit::test]
fn headers_neither_move_nor_run(cx: &mut TestAppContext) {
    let center = open(cx, props(mixed_rows()));
    cx.update_window(center.window, |_, window, cx| {
        window
            .within(ROOT)
            .click(("command-center-row", 2usize), cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert!(center.events().is_empty());
}

#[gpui_kit::test]
fn a_click_outside_dismisses(cx: &mut TestAppContext) {
    let center = open(cx, props(mixed_rows()));
    dispatch(
        cx,
        center.window,
        down(
            MouseButton::Left,
            point(px(20.), px(580.)),
            Modifiers::default(),
        ),
    );
    assert_eq!(center.events(), [CommandCenterEvent::Dismissed]);
}

#[gpui_kit::test]
fn the_argument_step_shows_a_chip(cx: &mut TestAppContext) {
    let mut step = props(Vec::new());
    step.argument = Some(Argument {
        label: "Go to folder".into(),
        kind: ArgKind::Path,
    });
    let center = open(cx, step);
    assert_eq!(
        center.label("command-center-chip", cx).as_deref(),
        Some("Go to folder")
    );
}

#[gpui_kit::test]
fn the_footer_counts_every_match_and_shows_the_keys(cx: &mut TestAppContext) {
    let center = open(cx, props(mixed_rows()));
    cx.update_window(center.window, |_, window, cx| {
        center
            .center
            .update(cx, |center, cx| center.set_query("cop", window, cx));
    })
    .expect("window");
    assert_eq!(
        center.label("command-center-note", cx).as_deref(),
        Some("9 results")
    );
    assert_eq!(
        center.label("command-center-hints", cx).as_deref(),
        Some("Enter run · Ctrl+Down next group · Esc close · Tab mode")
    );
}

#[gpui_kit::test]
fn an_error_replaces_the_count(cx: &mut TestAppContext) {
    let mut failed = props(Vec::new());
    failed.error = Some("No folder at \"C:\\nope\".".into());
    let center = open(cx, failed);
    assert_eq!(
        center.label("command-center-error", cx).as_deref(),
        Some("No folder at \"C:\\nope\".")
    );
}

#[gpui_kit::test]
fn a_scoped_list_of_500_rows_is_virtual(cx: &mut TestAppContext) {
    let hits = (0..500)
        .map(|n| file(&format!("budget_{n}.xlsx")))
        .collect();
    let center = open(cx, props(Results::merge(hits, Scope::Files)));
    let rows = center.rows(cx);
    assert_eq!(rows[0], "# Files");
    assert!(
        rows.len() < 40,
        "only visible rows render, got {}",
        rows.len()
    );
}

fn keys(list: &[Hint]) -> Vec<&'static str> {
    list.iter().map(|hint| hint.keys).collect()
}

#[test]
fn hints_follow_the_selected_row_kind() {
    let rows = mixed_rows();
    let files = [Row::Hit(file("a.pdf"))];
    let show_all = [Row::ShowAll {
        group: Group::Files,
        total: 20,
    }];
    assert_eq!(
        keys(&hints(rows.get(1), None)),
        ["enter", "ctrl-down", "escape", "tab"]
    );
    assert_eq!(
        keys(&hints(files.first(), None)),
        ["enter", "shift-enter", "ctrl-enter", "alt-enter", "tab"]
    );
    assert_eq!(
        keys(&hints(show_all.first(), None)),
        ["enter", "ctrl-down", "tab"]
    );
    assert_eq!(keys(&hints(None, None)), ["escape", "tab"]);
    assert_eq!(
        keys(&hints(None, Some(ArgKind::Path))),
        ["enter", "tab", "escape"]
    );
    assert_eq!(keys(&hints(None, Some(ArgKind::Text))), ["enter", "escape"]);
}

#[test]
fn the_count_includes_rows_behind_show_all() {
    assert_eq!(result_count(&mixed_rows()), 9);
    assert_eq!(result_count(&[Row::Header(Heading::Recent), Row::Hint]), 0);
}

#[test]
fn highlights_cover_matched_characters_as_byte_runs() {
    assert_eq!(highlight_ranges("Copy path", &[0, 1, 2, 5]), [0..3, 5..6]);
    assert_eq!(highlight_ranges("Ärger.txt", &[0, 1]), vec![0..3]);
    assert!(highlight_ranges("ab", &[7]).is_empty());
}

#[test]
fn highlights_land_on_the_matched_letters_after_combining_marks_and_emoji() {
    let cases = [
        ("re\u{301}sume\u{301}.txt", "sum"),
        (
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} family.txt",
            "fam",
        ),
    ];
    for (name, query) in cases {
        let found = rusty_index::fuzzy::Fuzzy::new(query, false)
            .score(name)
            .expect("match");
        let shown: String = highlight_ranges(name, &found.indices)
            .into_iter()
            .map(|range| &name[range])
            .collect();
        assert_eq!(shown, query);
    }
}

#[test]
fn the_panel_centres_on_the_field_and_stays_inside_the_window() {
    let field = Bounds::new(point(px(380.), px(7.)), size(px(520.), px(32.)));
    let frame = place(field, size(px(1280.), px(720.)));
    assert_eq!(frame.width, px(640.));
    assert_eq!(frame.origin.x, px(320.));
    assert_eq!(frame.origin.y, px(2.));
    let narrow = place(field, size(px(600.), px(720.)));
    assert_eq!(narrow.width, px(568.));
    assert_eq!(narrow.origin.x, px(16.));
}

#[gpui_kit::test]
fn key_hints_end_before_the_scrollbar_of_a_long_list(cx: &mut TestAppContext) {
    let hits = (0..70).map(|n| Hit {
        score: 100 - n,
        ..command(&format!("Copy {n}"), Some("ctrl-v"))
    });
    let center = open(cx, props(Results::merge(hits.collect(), Scope::Commands)));
    let (key, list) = center.read(cx, |window| {
        let key = window
            .within(("command-center-row", 1usize))
            .find(("command-center-key", 0usize))
            .bounds();
        (key, window.find("command-center-list").bounds())
    });
    assert!(key.right() <= list.right() - Scrollbar::width());
}

#[gpui_kit::test]
fn key_caps_are_written_with_capitals(cx: &mut TestAppContext) {
    let hits = vec![
        command("Paste", Some("ctrl-v")),
        Hit {
            score: 9,
            ..command("Back", Some("alt-left"))
        },
    ];
    let center = open(cx, props(Results::merge(hits, Scope::Commands)));
    let keys = center.read(cx, |window| {
        (1usize..3)
            .map(|row| {
                let key = window
                    .within(("command-center-row", row))
                    .find(("command-center-key", 0usize));
                key.label().map(str::to_owned)
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(
        keys,
        [Some("Ctrl+V".to_owned()), Some("Alt+Left".to_owned())]
    );
}
