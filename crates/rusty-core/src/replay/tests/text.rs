use super::*;

const SAMPLE: &str = r#"ferrite-log 1
home "/home/admin"
+0 pane.navigate { location: "/home/admin/src" }
+12 !listing-loaded { tab: 3, request: 1, location: "/home/admin/src", entries: 2 }
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
+0 !listing-loaded { tab: 9, request: 1, location: "/tmp/é \"x\"\\y", entries: 0 }
+1000000 !listing-loaded { tab: 10, request: 1, location: "/tmp", entries: 2 }
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
        "+3 !listing-loaded { tab: 3, request: 1, location: \"/h\", entries: 3 }\n",
        "  folder \"a\"\n",
        "  file \"b\"\n",
        "+1 selection.toggle\n",
    );
    assert_eq!(error_line(&with_body(body)), Some(4));
}

#[test]
fn a_huge_entry_count_is_an_error_not_a_crash() {
    let body = "+3 !listing-loaded { tab: 3, request: 1, location: \"/h\", entries: 18446744073709551615 }\n";
    assert_eq!(error_line(&with_body(body)), Some(4));
}

#[test]
fn a_missing_home_line_names_line_two() {
    assert_eq!(error_line("ferrite-log 1\n"), Some(2));
}

#[test]
fn a_bad_home_line_names_line_two() {
    assert_eq!(error_line("ferrite-log 1\nhome 5\n"), Some(2));
}

#[test]
fn an_empty_home_names_line_two() {
    assert_eq!(error_line("ferrite-log 1\nhome \"\"\n"), Some(2));
}

#[test]
fn a_version_below_one_is_a_bad_header() {
    assert_eq!(CommandLog::parse(&header("0")), Err(LogError::BadHeader));
}

#[test]
fn a_record_without_plus_names_its_line() {
    assert_eq!(error_line(&with_body("3 pane.back\n")), Some(4));
}

#[test]
fn a_record_without_a_space_names_its_line() {
    assert_eq!(error_line(&with_body("+3\n")), Some(4));
}

#[test]
fn a_bad_time_names_its_line() {
    assert_eq!(error_line(&with_body("+x pane.back\n")), Some(4));
}

#[test]
fn an_unknown_event_names_its_line() {
    assert_eq!(error_line(&with_body("+1 !resized { pane: 2 }\n")), Some(4));
}

#[test]
fn an_unknown_entry_kind_names_the_announcing_line() {
    let body =
        "+1 !listing-loaded { tab: 3, request: 1, location: \"/h\", entries: 1 }\n  link \"a\"\n";
    assert_eq!(error_line(&with_body(body)), Some(4));
}

#[test]
fn an_entry_line_without_indent_names_the_announcing_line() {
    let body =
        "+1 !listing-loaded { tab: 3, request: 1, location: \"/h\", entries: 1 }\nfile \"a\"\n";
    assert_eq!(error_line(&with_body(body)), Some(4));
}

#[test]
fn a_string_for_a_number_names_its_line() {
    assert_eq!(
        error_line(&with_body("+1 !page-rows { pane: \"2\", rows: 3 }\n")),
        Some(4)
    );
}

#[test]
fn a_number_for_a_string_names_its_line() {
    let body = "+1 !listing-loaded { tab: 3, request: 1, location: 4, entries: 0 }\n";
    assert_eq!(error_line(&with_body(body)), Some(4));
}

#[test]
fn a_negative_number_names_its_line() {
    assert_eq!(
        error_line(&with_body("+1 !page-rows { pane: -2, rows: 3 }\n")),
        Some(4)
    );
}

#[test]
fn text_after_an_event_names_its_line() {
    assert_eq!(
        error_line(&with_body("+1 !page-rows { pane: 2, rows: 3 } x\n")),
        Some(4)
    );
}

#[test]
fn a_malformed_field_list_names_its_line() {
    assert_eq!(
        error_line(&with_body("+1 !page-rows { pane 2 }\n")),
        Some(4)
    );
}

#[test]
fn a_trimmed_log_writes_and_reads_its_dropped_count() {
    let mut log = CommandLog::new(loc("/home"));
    for _ in 0..CommandLog::LIMIT + 3 {
        log.push(1, run(Command::PaneBack));
    }
    let text = log.to_text();
    assert!(text.lines().nth(2) == Some("dropped 3"), "{}", &text[..60]);
    assert_eq!(CommandLog::parse(&text).map(|l| l.dropped()), Ok(3));
}

#[test]
fn a_complete_log_has_no_dropped_line() {
    assert!(!sample().to_text().contains("\ndropped "));
}

#[test]
fn a_bad_dropped_count_names_line_three() {
    assert_eq!(
        error_line("ferrite-log 1\nhome \"/h\"\ndropped x\n"),
        Some(3)
    );
}

#[test]
fn a_filtered_listing_round_trips_through_text() {
    let text = with_body(
        "+7 !listing-filtered { tab: 3, location: \"/h\", entries: 1 }\n  file \"pa.rs\"\n",
    );
    let log = CommandLog::parse(&text).unwrap();
    assert_eq!(log.to_text(), text);
}

#[test]
fn a_cached_listing_round_trips_through_text() {
    let text = with_body(
        "+3 !listing-cached { tab: 3, request: 4, location: \"/h\", entries: 1 }\n  folder \"src\"\n",
    );
    let log = CommandLog::parse(&text).unwrap();
    assert!(matches!(
        log.records().last().map(|record| &record.entry),
        Some(LogEntry::Event(Event::ListingCached { request: 4, .. }))
    ));
    assert_eq!(log.to_text(), text);
}
