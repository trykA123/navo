use std::cell::Cell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use rusty_ui::window_view::{StartReport, StartWatch};

use super::clock::ProcessClock;
use super::marks;
use super::memory::Memory;
use super::report::{Build, Measured, Report};

pub const IDLE: Duration = Duration::from_secs(5);

#[derive(Clone)]
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
        let run = self.clone();
        StartWatch::new(IDLE, move |ui| run.finish(ui))
    }

    pub fn finish(&self, ui: StartReport) {
        let mut all = marks::all();
        all.extend(ui.marks);
        all.push(("first frame", ui.first_frame));
        all.push(("first usable", ui.first_usable));
        all.sort_by_key(|(_, at)| *at);
        let measured = Measured {
            renderer: ui.renderer,
            first_frame: ui.first_frame,
            first_usable: ui.first_usable,
            marks: all,
            memory: Memory::sample(),
        };
        let report = Report::new(Build::current(), &self.clock, measured);
        match report.write(&self.destination) {
            Ok(()) => self.written.set(true),
            Err(error) => eprintln!(
                "ferrite: cannot write the metrics to {}: {error}",
                self.destination.display()
            ),
        }
    }

    pub fn exit_code(&self) -> ExitCode {
        if self.is_written() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }

    pub fn is_written(&self) -> bool {
        self.written.get()
    }
}
