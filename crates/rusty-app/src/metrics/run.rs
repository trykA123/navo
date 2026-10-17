use std::cell::Cell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use rusty_ui::window_view::{StartReport, StartWatch};

use super::clock::ProcessClock;

pub const IDLE: Duration = Duration::from_secs(5);

pub struct Run {
    destination: PathBuf,
    clock: ProcessClock,
    written: Rc<Cell<bool>>,
}

impl Run {
    pub fn new(destination: PathBuf, clock: ProcessClock) -> Self {
        Self {
            destination,
            clock,
            written: Rc::default(),
        }
    }

    pub fn watch(&self) -> StartWatch {
        StartWatch::new(IDLE, |_| {})
    }

    pub fn finish(&self, ui: StartReport) {
        let _ = (ui, &self.destination, self.clock);
    }

    pub fn exit_code(&self) -> ExitCode {
        let _ = self.written.get();
        ExitCode::SUCCESS
    }

    pub fn is_written(&self) -> bool {
        self.written.get()
    }
}
