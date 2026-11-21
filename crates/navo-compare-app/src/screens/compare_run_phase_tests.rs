//! Tests that draining a compare run feeds the D5 phase meters from the engine updates.

use std::sync::{Arc, Mutex};

use navo_compare_cli::args::SideSpec;
use sidewise_core::Location;
use sidewise_ui::browser::{
    CompareControl, CompareHooks, ComparePhase, CompareSink, CompareUpdate, Waker,
};
use sidewise_ui::compare_diff::CompareOptions;

use super::compare_run::CompareRun;
use super::large_model::Meter;
use crate::start_tabs::StartChoice;

/// A hooks stand-in that sends a fixed list of updates, then reports an idle compare.
struct Scripted(Vec<CompareUpdate>);

impl CompareHooks for Scripted {
    /// Every location can be compared in this fixture.
    fn supports(&self, _location: &Location) -> bool {
        true
    }

    /// Sends the scripted updates to the sink and returns an idle control.
    fn start(
        &self,
        _sides: (&Location, &Location),
        _options: &CompareOptions,
        sink: CompareSink,
    ) -> Box<dyn CompareControl> {
        for update in &self.0 {
            sink(update.clone());
        }
        Box::new(Idle)
    }
}

/// A compare control that ignores every request.
struct Idle;

impl CompareControl for Idle {
    /// Ignores a focus request.
    fn focus(&self, _row_ids: Vec<u64>) {}

    /// Ignores a stop request for one row.
    fn stop_row(&self, _id: u64) {}

    /// Ignores a stop request for the whole walk.
    fn stop_all(&self) {}

    /// Ignores a refresh request.
    fn refresh(&self) {}

    /// Ignores a request to resume a stopped walk.
    fn continue_walk(&self) {}
}

/// A drained `PhaseProgress` update writes the listing meter.
#[test]
fn drain_feeds_phase_meters() {
    let pid = std::process::id();
    let left = std::env::temp_dir().join(format!("sw-h1b-{pid}-left"));
    let right = std::env::temp_dir().join(format!("sw-h1b-{pid}-right"));
    std::fs::create_dir_all(&left).expect("create left folder");
    std::fs::create_dir_all(&right).expect("create right folder");

    let hooks = Scripted(vec![CompareUpdate::PhaseProgress {
        generation: 1,
        phase: ComparePhase::Listing,
        done: 3,
        total: None,
    }]);
    let wake: Waker = Arc::new(|| {});
    let mut run = CompareRun::start(
        (
            &SideSpec::Path(left.clone()),
            &SideSpec::Path(right.clone()),
        ),
        &StartChoice::default(),
        &hooks,
        wake,
    )
    .expect("start the scripted run");

    assert!(run.drain());
    assert_eq!(
        run.meters().meter(ComparePhase::Listing),
        Meter {
            done: 3,
            total: None
        }
    );

    std::fs::remove_dir_all(&left).expect("remove left folder");
    std::fs::remove_dir_all(&right).expect("remove right folder");
}

/// A hooks stand-in that records the `fresh_hashes` flag of every start.
struct Recording(Arc<Mutex<Vec<bool>>>);

impl CompareHooks for Recording {
    /// Every location can be compared in this fixture.
    fn supports(&self, _location: &Location) -> bool {
        true
    }

    /// Records the `fresh_hashes` option and returns an idle control.
    fn start(
        &self,
        _sides: (&Location, &Location),
        options: &CompareOptions,
        _sink: CompareSink,
    ) -> Box<dyn CompareControl> {
        self.0
            .lock()
            .expect("lock the list")
            .push(options.fresh_hashes);
        Box::new(Idle)
    }
}

/// `start` asks for cached hashes; `restart(.., true)` asks for fresh ones.
#[test]
fn restart_with_recheck_asks_for_fresh_hashes() {
    let pid = std::process::id();
    let left = std::env::temp_dir().join(format!("sw-u1-{pid}-left"));
    let right = std::env::temp_dir().join(format!("sw-u1-{pid}-right"));
    std::fs::create_dir_all(&left).expect("create left folder");
    std::fs::create_dir_all(&right).expect("create right folder");

    let seen = Arc::new(Mutex::new(Vec::new()));
    let hooks = Recording(Arc::clone(&seen));
    let wake: Waker = Arc::new(|| {});
    let mut run = CompareRun::start(
        (
            &SideSpec::Path(left.clone()),
            &SideSpec::Path(right.clone()),
        ),
        &StartChoice::default(),
        &hooks,
        wake,
    )
    .expect("start the recording run");
    run.restart(&hooks, true);

    assert_eq!(*seen.lock().expect("lock the list"), vec![false, true]);

    std::fs::remove_dir_all(&left).expect("remove left folder");
    std::fs::remove_dir_all(&right).expect("remove right folder");
}
