use super::super::find_memory::MAX_FINDS;
use crate::placement::{Monitor, MonitorLocalLogicalRect, MonitorSetup, PhysicalRect, Placement};
use crate::{AppState, FindMemory, Location, SavedFind};

fn home() -> AppState {
    AppState::new(Location::new("/home").unwrap())
}

fn monitor(bounds: PhysicalRect, scale: f32) -> Monitor {
    Monitor {
        bounds,
        work_area: bounds,
        scale,
        primary: false,
    }
}

fn setup(scale: f32) -> MonitorSetup {
    MonitorSetup::new(vec![monitor(
        PhysicalRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
        scale,
    )])
    .unwrap()
}

fn placement(setup: &MonitorSetup, maximized: bool) -> Placement {
    Placement {
        setup: setup.fingerprint(),
        bounds: MonitorLocalLogicalRect {
            x: 10.0,
            y: 20.0,
            width: 800.0,
            height: 600.0,
        },
        monitor: setup.monitors()[0].bounds,
        maximized,
    }
}

fn saved(text: &str) -> SavedFind {
    SavedFind {
        text: text.to_owned(),
        base: Location::new("/home").unwrap(),
    }
}

fn texts(memory: &FindMemory) -> Vec<&str> {
    memory.queries().iter().map(|q| q.text.as_str()).collect()
}

#[test]
fn set_queries_keeps_the_last_twelve() {
    let mut memory = FindMemory::default();
    memory.set_queries((0..14).map(|i| saved(&format!("q{i}"))).collect());
    assert_eq!(memory.queries().len(), MAX_FINDS);
    assert_eq!(texts(&memory)[0], "q2");
    assert_eq!(texts(&memory)[MAX_FINDS - 1], "q13");
}

#[test]
fn set_queries_drops_whitespace_only_text() {
    let mut memory = FindMemory::default();
    memory.set_queries(vec![saved(" "), saved("needle"), saved("\t")]);
    assert_eq!(texts(&memory), ["needle"]);
}

#[test]
fn a_repeated_query_is_kept_once_at_the_newest_slot() {
    let mut memory = FindMemory::default();
    memory.set_queries(vec![saved("a"), saved("b"), saved("a")]);
    assert_eq!(texts(&memory), ["b", "a"]);
}

#[test]
fn the_same_text_in_another_folder_is_kept() {
    let mut memory = FindMemory::default();
    let mut other = saved("a");
    other.base = Location::new("/other").unwrap();
    memory.set_queries(vec![saved("a"), other]);
    assert_eq!(memory.queries().len(), 2);
}

#[test]
fn set_queries_drops_empty_text() {
    let mut memory = FindMemory::default();
    memory.set_queries(vec![saved(""), saved("needle"), saved("")]);
    assert_eq!(texts(&memory), ["needle"]);
}

#[test]
fn placement_is_kept_per_monitor_setup() {
    let mut memory = FindMemory::default();
    let one = setup(1.0);
    let two = setup(2.0);
    memory.remember(&one, placement(&one, false));
    memory.remember(&two, placement(&two, true));
    assert_eq!(memory.placement().len(), 2);
    assert_eq!(memory.placement()[0].setup, two.fingerprint());
    assert!(memory.placement()[0].maximized);

    memory.remember(&one, placement(&one, false));
    assert_eq!(memory.placement().len(), 2);
    assert_eq!(memory.placement()[0].setup, one.fingerprint());
    assert_eq!(memory.placement()[1].setup, two.fingerprint());
}

#[test]
fn is_empty_reflects_placement_pin_and_queries() {
    let mut memory = FindMemory::default();
    assert!(memory.is_empty());

    memory.pinned = true;
    assert!(!memory.is_empty());
    memory.pinned = false;

    memory.set_queries(vec![saved("needle")]);
    assert!(!memory.is_empty());

    let mut placed = FindMemory::default();
    let one = setup(1.0);
    placed.remember(&one, placement(&one, false));
    assert!(!placed.is_empty());
    assert!(placed.queries().is_empty());
}

#[test]
fn app_state_reaches_the_focused_windows_memory() {
    let mut state = home();
    assert!(state.find_memory().is_empty());
    state.find_memory_mut().pinned = true;
    assert!(state.find_memory().pinned);

    state.open_window(Location::new("/b").unwrap());
    assert!(!state.find_memory().pinned);
}
