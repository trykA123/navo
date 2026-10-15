use super::*;

const SAMPLE: &str = r#"ferrite-log 1
home "/home/admin"
+0 pane.navigate { location: "/home/admin/src" }
+12 !listing-loaded { tab: 3, location: "/home/admin/src", entries: 2 }
  folder "lib"
  file "main.rs"
+250 !page-rows { pane: 2, rows: 30 }
+40 selection.toggle
+5 pane.open-tab
+5 pane.open-tab { location: "C:\\Users\\admin \"q\"" }
+5 pane.close-tab
+5 pane.close-tab { tab: 1 }
+5 pane.back
+5 pane.forward
+5 pane.cursor-move { to: page-down }
+5 pane.cursor-move { to: 7 }
+5 selection.toggle-down
+5 selection.range { to: 3 }
+5 selection.add-pattern { pattern: "*.rs" }
+5 selection.remove-pattern { pattern: "m*" }
+5 selection.invert
+5 selection.all
+5 selection.clear
+5 layout.split { direction: right }
+5 layout.focus { direction: left }
+5 layout.close
+5 app.save-session
+0 !listing-loaded { tab: 9, location: "/tmp/é \"x\"\\y", entries: 0 }
+1000000 !listing-loaded { tab: 10, location: "/tmp", entries: 2 }
  file "tab\there \"q\""
  folder "中\u{1b}📁"
"#;

fn every_command_kind() -> Vec<Command> {
    vec![
        Command::PaneOpenTab { location: None },
        Command::PaneOpenTab {
            location: Some(loc("C:\\Users\\admin \"q\"")),
        },
        Command::PaneCloseTab { tab: None },
        Command::PaneCloseTab { tab: Some(1) },
        Command::PaneBack,
        Command::PaneForward,
        Command::PaneCursorMove {
            to: CursorMove::PageDown,
        },
        Command::PaneCursorMove {
            to: CursorMove::Row(7),
        },
        Command::SelectionToggleDown,
        Command::SelectionRange { to: 3 },
        Command::SelectionAddPattern {
            pattern: "*.rs".to_owned(),
        },
        Command::SelectionRemovePattern {
            pattern: "m*".to_owned(),
        },
        Command::SelectionInvert,
        Command::SelectionAll,
        Command::SelectionClear,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
        Command::LayoutFocus {
            direction: Direction::Left,
        },
        Command::LayoutClose,
        Command::AppSaveSession,
    ]
}

fn sample() -> CommandLog {
    let mut log = CommandLog::new(loc("/home/admin"));
    log.push(
        0,
        run(Command::PaneNavigate {
            location: loc("/home/admin/src"),
        }),
    );
    log.push(
        12,
        loaded(3, "/home/admin/src", &[("lib", true), ("main.rs", false)]),
    );
    log.push(250, page_rows(2, 30));
    log.push(40, run(Command::SelectionToggle));
    for command in every_command_kind() {
        log.push(5, run(command));
    }
    log.push(0, loaded(9, "/tmp/é \"x\"\\y", &[]));
    log.push(
        1_000_000,
        loaded(
            10,
            "/tmp",
            &[("tab\there \"q\"", false), ("中\u{1b}📁", true)],
        ),
    );
    log
}

fn header(version: &str) -> String {
    format!("ferrite-log {version}\nhome \"/home/admin\"\n+0 pane.back\n")
}

fn with_body(body: &str) -> String {
    format!("{}{body}", header("1"))
}

fn error_line(text: &str) -> Option<usize> {
    match CommandLog::parse(text) {
        Err(LogError::Line { line, .. }) => Some(line),
        _ => None,
    }
}

#[test]
fn a_log_prints_in_its_text_form() {
    let text = sample().to_text();
    insta::with_settings!({ snapshot_path => "../snapshots", prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!("log_text", text);
    });
}

#[test]
fn the_text_form_parses_back_to_the_log() {
    assert_eq!(CommandLog::parse(SAMPLE), Ok(sample()));
}

#[test]
fn a_wrong_header_is_refused() {
    let text = header("1").replace("ferrite-log", "ferrite-lg");
    assert_eq!(CommandLog::parse(&text), Err(LogError::BadHeader));
}

#[test]
fn a_newer_version_is_refused() {
    assert_eq!(
        CommandLog::parse(&header("2")),
        Err(LogError::TooNew {
            found: 2,
            supported: CommandLog::VERSION,
        })
    );
}

#[test]
fn a_bad_command_names_its_line() {
    assert_eq!(error_line(&with_body("+3 pane.fly\n")), Some(4));
}

#[test]
fn a_bad_event_names_its_line() {
    let body = "+3 selection.toggle\n+3 !page-rows { pane: 2 }\n";
    assert_eq!(error_line(&with_body(body)), Some(5));
}

#[test]
fn a_listing_with_missing_entries_names_the_line_that_announced_them() {
    let body = concat!(
        "+3 !listing-loaded { tab: 3, location: \"/h\", entries: 3 }\n",
        "  folder \"a\"\n",
        "  file \"b\"\n",
        "+1 selection.toggle\n",
    );
    assert_eq!(error_line(&with_body(body)), Some(4));
}
