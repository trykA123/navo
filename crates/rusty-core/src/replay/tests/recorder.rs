use super::*;
use crate::{CommandError, StateError};

fn recorder() -> Recorder {
    Recorder::new(loc("/home"))
}

fn session() -> Vec<Command> {
    vec![
        Command::SelectionToggle,
        Command::PaneNavigate {
            location: loc("/work"),
        },
        Command::PaneBack,
        Command::PaneBack,
        Command::LayoutSplit {
            direction: Direction::Right,
        },
        Command::AppSaveSession,
    ]
}

#[test]
fn failed_commands_are_recorded_too() {
    let mut recorder = recorder();
    let _ = recorder.command(7, Command::PaneBack);
    assert_eq!(
        recorder.log().records(),
        [Record {
            after_ms: 0,
            entry: run(Command::PaneBack),
        }]
    );
}

#[test]
fn a_failed_command_returns_the_error_of_apply() {
    let mut recorder = recorder();
    assert_eq!(
        recorder.command(0, Command::PaneBack),
        Err(CommandError::State(StateError::NothingBack))
    );
}

#[test]
fn commands_return_what_apply_returns() {
    let mut recorder = recorder();
    let mut state = AppState::new(loc("/home"));
    for (at_ms, command) in (0..).zip(session()) {
        assert_eq!(
            recorder.command(at_ms, command.clone()),
            apply(&mut state, &command),
            "{command:?}"
        );
    }
}

#[test]
fn commands_change_the_state_as_apply_does() {
    let mut recorder = recorder();
    let mut state = AppState::new(loc("/home"));
    for (at_ms, command) in (0..).zip(session()) {
        let _ = recorder.command(at_ms, command.clone());
        let _ = apply(&mut state, &command);
    }
    assert_eq!(*recorder.state(), state);
}

#[test]
fn events_return_and_change_what_apply_event_does() {
    let mut recorder = recorder();
    let mut state = AppState::new(loc("/home"));
    let events = [
        Event::ListingLoaded {
            tab: TabId::new(3),
            location: loc("/home"),
            listing: listing(&[("a", true), ("b", false)]),
        },
        Event::PageRowsChanged {
            pane: PaneId::new(2),
            rows: 12,
        },
    ];
    let mut returned = Vec::new();
    let mut expected = Vec::new();
    for (at_ms, event) in (0..).zip(events) {
        returned.push(recorder.event(at_ms, event.clone()));
        expected.push(apply_event(&mut state, event));
    }
    assert_eq!((returned, recorder.state()), (expected, &state));
}

#[test]
fn events_are_recorded() {
    let mut recorder = recorder();
    let event = Event::PageRowsChanged {
        pane: PaneId::new(2),
        rows: 12,
    };
    let _ = recorder.event(40, event.clone());
    assert_eq!(
        recorder.log().records(),
        [Record {
            after_ms: 0,
            entry: LogEntry::Event(event),
        }]
    );
}

#[test]
fn records_store_the_time_since_the_previous_record() {
    let mut recorder = recorder();
    for at_ms in [100, 130, 130, 1130] {
        let _ = recorder.command(at_ms, Command::SelectionClear);
    }
    assert_eq!(after_ms(recorder.log()), [0, 30, 0, 1000]);
}

#[test]
fn a_clock_going_backwards_records_zero_and_counts_on_from_there() {
    let mut recorder = recorder();
    let _ = recorder.command(500, Command::SelectionClear);
    let _ = recorder.command(200, Command::SelectionClear);
    let _ = recorder.event(
        260,
        Event::PageRowsChanged {
            pane: PaneId::new(2),
            rows: 5,
        },
    );
    assert_eq!(after_ms(recorder.log()), [0, 0, 60]);
}

#[test]
fn a_full_log_keeps_the_newest_records() {
    let mut log = CommandLog::new(loc("/home"));
    let total = CommandLog::LIMIT as u64 + 5;
    for i in 0..total {
        log.push(i, run(Command::SelectionToggle));
    }
    assert_eq!(after_ms(&log), (5..total).collect::<Vec<_>>());
}
