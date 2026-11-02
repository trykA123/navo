use super::*;

fn with_body(body: &str) -> String {
    format!("navo-log 1\nhome \"/h\"\n{body}")
}

#[test]
fn restored_marks_round_trip_through_text() {
    let text =
        with_body("+7 !marks-restored { tab: 3, names: 2 }\n  \"a.rs\"\n  \"q \\\"x\\\"\"\n");
    let log = CommandLog::parse(&text).unwrap();
    assert!(matches!(
        log.records().last().map(|record| &record.entry),
        Some(LogEntry::Event(Event::MarksRestored { names, .. })) if names.len() == 2
    ));
    assert_eq!(log.to_text(), text);
}

#[test]
fn restored_marks_with_missing_name_lines_are_refused() {
    let text = with_body("+7 !marks-restored { tab: 3, names: 2 }\n  \"a.rs\"\n");
    assert!(CommandLog::parse(&text).is_err());
}

#[test]
fn replaying_restored_marks_marks_the_rows() {
    let mut recorder = Recorder::new(loc("/h"));
    let tab = recorder.state().active_tab().id();
    recorder.command(0, Command::PaneRefresh).unwrap();
    let request = recorder.state().active_tab().pending_request().unwrap();
    let event = Event::ListingLoaded {
        tab,
        location: loc("/h"),
        request,
        listing: listing(&[("a", false), ("b", false), ("c", false)]),
    };
    recorder.event(1, event);
    let names = vec!["b".to_owned(), "c".to_owned()];
    recorder.event(2, Event::MarksRestored { tab, names });
    let state = recorder.log().replay();
    assert_eq!(state.active_tab().selection().marked(), vec![1, 2]);
    assert_eq!(&state, recorder.state());
}
