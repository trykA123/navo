use proptest::prelude::*;

use super::*;
use crate::Effect;

const POOL: [&str; 6] = [
    "/home",
    "/a \"quoted\"",
    "C:\\Users\\admin\\",
    "/tab\there\nline",
    "/é/中/📁\u{200d}",
    "/ctl\u{1}\u{1b}\u{7f}\u{85}",
];

#[derive(Debug, Clone)]
enum Step {
    Run(Command),
    Answer { pick: usize, listing: Listing },
    Stale { tab: u64, listing: Listing },
    PageRows { pick: usize, rows: u64 },
}

fn location() -> impl Strategy<Value = Location> {
    prop::sample::select(POOL.to_vec()).prop_map(loc)
}

fn direction() -> impl Strategy<Value = Direction> {
    prop::sample::select(Direction::ALL.to_vec())
}

fn cursor_move() -> impl Strategy<Value = CursorMove> {
    prop_oneof![
        Just(CursorMove::Up),
        Just(CursorMove::Down),
        Just(CursorMove::PageUp),
        Just(CursorMove::PageDown),
        Just(CursorMove::First),
        Just(CursorMove::Last),
        (0..40_u64).prop_map(CursorMove::Row),
    ]
}

fn text() -> impl Strategy<Value = String> {
    let tricky = prop_oneof![
        Just('"'),
        Just('\\'),
        Just('\n'),
        Just('\t'),
        Just('\u{1b}'),
        Just('{'),
        Just(','),
        Just(' '),
        Just('é'),
        Just('📁'),
        Just('*'),
        Just('a'),
        any::<char>().prop_filter("no NUL", |c| *c != '\0'),
    ];
    prop::collection::vec(tricky, 0..8).prop_map(|cs| cs.into_iter().collect())
}

fn command() -> impl Strategy<Value = Command> {
    prop_oneof![
        2 => prop::option::of(location()).prop_map(|location| Command::PaneOpenTab { location }),
        2 => prop::option::of(0..4_u32).prop_map(|tab| Command::PaneCloseTab { tab }),
        3 => location().prop_map(|location| Command::PaneNavigate { location }),
        2 => Just(Command::PaneBack),
        2 => Just(Command::PaneForward),
        3 => cursor_move().prop_map(|to| Command::PaneCursorMove { to }),
        3 => Just(Command::SelectionToggle),
        2 => Just(Command::SelectionToggleDown),
        2 => (0..40_u64).prop_map(|to| Command::SelectionRange { to }),
        2 => (text(), any::<bool>()).prop_map(|(pattern, folders)| Command::SelectionAddPattern { pattern, folders }),
        1 => (text(), any::<bool>()).prop_map(|(pattern, folders)| Command::SelectionRemovePattern { pattern, folders }),
        1 => Just(Command::SelectionInvert),
        1 => Just(Command::SelectionAll),
        1 => Just(Command::SelectionClear),
        2 => direction().prop_map(|direction| Command::LayoutSplit { direction }),
        2 => Just(Command::LayoutClose),
        2 => direction().prop_map(|direction| Command::LayoutFocus { direction }),
        1 => Just(Command::AppSaveSession),
    ]
}

fn any_listing() -> impl Strategy<Value = Listing> {
    prop::collection::vec((text(), any::<bool>()), 0..=12).prop_map(|rows| {
        Listing::new(
            rows.into_iter()
                .map(|(name, is_folder)| Entry { name, is_folder })
                .collect(),
        )
    })
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        8 => command().prop_map(Step::Run),
        4 => (any::<usize>(), any_listing())
            .prop_map(|(pick, listing)| Step::Answer { pick, listing }),
        1 => (0..16_u64, any_listing()).prop_map(|(tab, listing)| Step::Stale { tab, listing }),
        1 => (any::<usize>(), 0..30_u64).prop_map(|(pick, rows)| Step::PageRows { pick, rows }),
    ]
}

fn clock() -> impl Strategy<Value = u64> {
    prop_oneof![0..10_000_u64, any::<u64>()]
}

fn pending_folders(effects: &[Effect]) -> impl Iterator<Item = (TabId, Location, u64)> + '_ {
    effects.iter().filter_map(|effect| match effect {
        Effect::ListFolder {
            tab,
            location,
            request,
        } => Some((*tab, location.clone(), *request)),
        _ => None,
    })
}

fn event(recorder: &Recorder, pending: &[(TabId, Location, u64)], step: Step) -> Option<Event> {
    let state = recorder.state();
    match step {
        Step::Run(_) => None,
        Step::Answer { pick, listing } => {
            let active = state.active_tab();
            let (tab, location, request) = pending
                .get(pick % pending.len().max(1))
                .cloned()
                .unwrap_or_else(|| (active.id(), active.location().clone(), 0));
            Some(Event::ListingLoaded {
                tab,
                location,
                request,
                listing,
            })
        }
        Step::Stale { tab, listing } => Some(Event::ListingLoaded {
            tab: TabId::new(tab),
            location: loc(POOL[0]),
            request: tab,
            listing,
        }),
        Step::PageRows { pick, rows } => {
            let panes: Vec<PaneId> = state
                .window_ids()
                .into_iter()
                .filter_map(|id| state.window(id))
                .flat_map(|w| w.layout().panes())
                .collect();
            let pane = panes[pick % panes.len()];
            Some(Event::PageRowsChanged { pane, rows })
        }
    }
}

fn record(steps: Vec<(u64, Step)>) -> Recorder {
    let mut recorder = Recorder::new(loc(POOL[0]));
    let mut pending = Vec::new();
    for (at_ms, step) in steps {
        if let Step::Run(command) = &step {
            if let Ok(effects) = recorder.command(at_ms, command.clone()) {
                pending.extend(pending_folders(&effects));
            }
        } else if let Some(event) = event(&recorder, &pending, step) {
            let _ = recorder.event(at_ms, event);
        }
    }
    recorder
}

fn session() -> impl Strategy<Value = Vec<(u64, Step)>> {
    prop::collection::vec((clock(), step()), 0..=80)
}

fn near_log_text() -> impl Strategy<Value = String> {
    let line = prop_oneof![
        Just("ferrite-log 1".to_owned()),
        Just("home \"/h\"".to_owned()),
        Just("+0 pane.back".to_owned()),
        Just("+1 !listing-loaded { tab: 3, location: \"/h\", entries: 2 }".to_owned()),
        Just("+1 !page-rows { pane: 2, rows: 9 }".to_owned()),
        Just("  file \"x\"".to_owned()),
        "[+! a-z0-9{}:,\"\\\\.-]{0,40}",
    ];
    prop::collection::vec(line, 0..8).prop_map(|lines| lines.join("\n"))
}

proptest! {
    #[test]
    fn replaying_a_recorded_log_gives_the_recorded_state(steps in session()) {
        let recorder = record(steps);
        prop_assert_eq!(&recorder.log().replay(), recorder.state());
    }

    #[test]
    fn a_recorded_log_survives_its_text_form(steps in session()) {
        let recorder = record(steps);
        let log = recorder.log();
        prop_assert_eq!(CommandLog::parse(&log.to_text()), Ok(log.clone()));
    }

    #[test]
    fn parse_never_panics_on_arbitrary_text(text in any::<String>()) {
        let _ = CommandLog::parse(&text);
    }

    #[test]
    fn parse_never_panics_on_text_close_to_a_log(text in near_log_text()) {
        let _ = CommandLog::parse(&text);
    }
}
