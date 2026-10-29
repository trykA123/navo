use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use rusty_ui::settings::{Change, Field, Value, Values};
use rusty_ui::window_view::{OpenRequest, ResidentHooks};

use super::super::events::{ResidentConfig, ResidentEvent};
use super::{Host, Running, request_for};

type Log = Rc<RefCell<Vec<String>>>;

struct Fake {
    id: usize,
    taken: bool,
    stuck: Rc<Cell<bool>>,
    log: Log,
}

impl Running for Fake {
    fn key_taken(&self) -> bool {
        self.taken
    }

    fn has_tray(&self) -> bool {
        false
    }

    fn stop(&mut self) -> bool {
        !self.stuck.get()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.log.borrow_mut().push(format!("stop {}", self.id));
    }
}

fn host(values: &Values) -> (Host<Fake>, Log) {
    host_with(values, Rc::default())
}

fn host_with(values: &Values, stuck: Rc<Cell<bool>>) -> (Host<Fake>, Log) {
    let log = Log::default();
    let starts = log.clone();
    let host = Host::new(values, move |config: ResidentConfig| {
        let id = starts
            .borrow()
            .iter()
            .filter(|line| line.starts_with("start"))
            .count()
            + 1;
        starts.borrow_mut().push(format!("start {id}"));
        Fake {
            id,
            taken: config.keep_in_tray,
            stuck: stuck.clone(),
            log: starts.clone(),
        }
    });
    (host, log)
}

fn keep_in_tray(values: &Values, on: bool) -> Values {
    values.with(&Change {
        field: Field::KeepInTray,
        value: Value::Flag(on),
    })
}

#[test]
fn the_host_starts_the_resident_with_the_current_settings() {
    let (_host, log) = host(&Values::default());
    assert_eq!(*log.borrow(), ["start 1"]);
}

#[test]
fn a_tray_setting_change_stops_the_old_resident_before_starting_the_new_one() {
    let values = Values::default();
    let (host, log) = host(&values);
    host.apply(&keep_in_tray(&values, true));
    host.apply(&values.with(&Change::text(Field::SummonKey, "Ctrl+Alt+K")));
    assert_eq!(
        *log.borrow(),
        ["start 1", "stop 1", "start 2", "stop 2", "start 3"]
    );
}

#[test]
fn other_setting_changes_keep_the_resident_running() {
    let values = Values::default();
    let (host, log) = host(&values);
    host.apply(&values);
    host.apply(&values.with(&Change::show_hidden(true)));
    assert_eq!(*log.borrow(), ["start 1"]);
}

#[test]
fn key_taken_comes_from_the_running_resident() {
    let values = Values::default();
    let (host, _) = host(&values);
    assert!(!host.key_taken());
    host.apply(&keep_in_tray(&values, true));
    assert!(host.key_taken());
}

#[test]
fn summon_and_quit_become_open_requests() {
    let at = Instant::now();
    assert_eq!(
        request_for(ResidentEvent::Summon(at)),
        OpenRequest {
            summoned_at: Some(at),
            ..OpenRequest::default()
        }
    );
    assert!(request_for(ResidentEvent::Quit).quit);
}

#[test]
fn stop_ends_the_resident_once() {
    let (host, log) = host(&Values::default());
    host.stop();
    host.stop();
    assert_eq!(*log.borrow(), ["start 1", "stop 1"]);
}

#[test]
fn a_resident_that_cannot_stop_is_kept_and_no_second_one_starts() {
    let values = Values::default();
    let stuck = Rc::new(Cell::new(true));
    let (host, log) = host_with(&values, stuck.clone());
    host.apply(&keep_in_tray(&values, true));
    assert_eq!(*log.borrow(), ["start 1"]);
    assert!(!host.key_taken());
    stuck.set(false);
    host.apply(&keep_in_tray(&values, true));
    assert_eq!(*log.borrow(), ["start 1", "stop 1", "start 2"]);
    assert!(host.key_taken());
}
