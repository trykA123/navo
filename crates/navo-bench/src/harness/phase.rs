use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::privacy::SafeText;

/// Code recorded when a phase panics.
pub const PANIC_CODE: u32 = 101;

/// What a phase reports about itself.
#[derive(Clone, Copy, Debug)]
pub enum PhaseResult {
    Done,
    Failed(u32),
    NotMeasured(SafeText),
}

/// The recorded outcome. Reasons are fixed text, never paths.
#[derive(Clone, Copy, Debug)]
pub enum Verdict {
    Ok,
    Failed(u32),
    NotMeasured(SafeText),
}

pub struct Phase<'a> {
    pub name: SafeText,
    run: Box<dyn FnMut() -> PhaseResult + 'a>,
}

impl<'a> Phase<'a> {
    pub fn new(name: SafeText, run: impl FnMut() -> PhaseResult + 'a) -> Self {
        Self {
            name,
            run: Box::new(run),
        }
    }
}

/// Receives each phase start and verdict while the driver runs.
pub trait Observer {
    fn started(&mut self, name: SafeText);
    fn finished(&mut self, name: SafeText, verdict: Verdict);
}

/// Runs the phases in order. A failing or panicking phase never stops the rest.
pub fn drive(phases: Vec<Phase<'_>>, observer: &mut impl Observer) -> Vec<(SafeText, Verdict)> {
    let mut verdicts = Vec::with_capacity(phases.len());
    for mut phase in phases {
        observer.started(phase.name);
        let verdict = match catch_unwind(AssertUnwindSafe(&mut phase.run)) {
            Ok(PhaseResult::Done) => Verdict::Ok,
            Ok(PhaseResult::Failed(code)) => Verdict::Failed(code),
            Ok(PhaseResult::NotMeasured(reason)) => Verdict::NotMeasured(reason),
            Err(_) => Verdict::Failed(PANIC_CODE),
        };
        observer.finished(phase.name, verdict);
        verdicts.push((phase.name, verdict));
    }
    verdicts
}
