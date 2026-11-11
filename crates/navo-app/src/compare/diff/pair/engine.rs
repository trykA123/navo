//! The engine a pair worker runs after loading: large text or bytes, restarted on refresh (V38w).

use std::ops::Range;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Arc;
use std::sync::{Mutex, PoisonError};

use navo_compare::bytes::Align;
use navo_compare::model::Budget;
use navo_ui::compare_diff::{DiffSink, PairControl, PairKind, PairRequest};

use super::super::bytes::{ByteDiff, ByteInput};
use super::super::large::{LargeDiff, LargeInput};
use super::super::load::{LoadInput, Loaded};
use super::super::sources::{SPOOL_DIR, Sources};
use crate::compare::settings::CompareSettings;
use navo_store::StoreDirs;

/// What every pair engine needs: the rules' settings, a memory budget and whether it spools.
#[derive(Clone)]
pub(crate) struct EnginePlan {
    pub settings: CompareSettings,
    pub budget: Budget,
    /// True when a Git blob may need the on-disk spool.
    pub spool: bool,
}

impl EnginePlan {
    /// A plan for `settings`; `spool` asks the worker for the Git spool folder.
    pub(crate) fn new(settings: CompareSettings, spool: bool) -> Self {
        Self {
            budget: settings.budget(),
            settings,
            spool,
        }
    }
}

impl Default for EnginePlan {
    /// Default rules and budget, with no spool folder (local pairs need none).
    fn default() -> Self {
        Self::new(CompareSettings::default(), false)
    }
}

/// The running engine of one pair; `None` for text, image and one-sided pairs.
pub(crate) enum Running {
    Large(LargeDiff),
    Bytes {
        _diff: ByteDiff,
    },
    #[cfg(test)]
    Probe(Arc<Mutex<ProbeCalls>>),
}

/// What a test engine recorded; only the `Live` tests build one.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct ProbeCalls {
    lines: Vec<Range<u64>>,
    focus: Vec<u64>,
    acked: Vec<u64>,
}

/// The inputs one engine start needs; a struct keeps the call site readable.
pub(crate) struct EngineJob<'a> {
    pub request: &'a PairRequest,
    pub loaded: &'a Loaded,
    pub load: &'a LoadInput,
    pub plan: &'a EnginePlan,
    pub sink: &'a DiffSink,
}

impl EngineJob<'_> {
    /// Start the engine that the loaded kind asks for; `None` when the pair has none.
    pub(crate) fn running(&self) -> Option<Running> {
        match self.loaded.kind {
            PairKind::Large { .. } => Some(Running::Large(LargeDiff::start(
                self.request.clone(),
                large_input(self.request, self.load, self.plan),
                self.sink.clone(),
            ))),
            PairKind::Binary => {
                let left = self.load.left.clone()?;
                let right = self.load.right.clone()?;
                let align = if left.1.size == right.1.size {
                    Align::Offset
                } else {
                    Align::Content
                };
                Some(Running::Bytes {
                    _diff: ByteDiff::start(
                        self.request.clone(),
                        ByteInput {
                            left,
                            right,
                            align,
                            run: 0,
                            budget: self.plan.budget,
                        },
                        self.sink.clone(),
                    ),
                })
            }
            _ => None,
        }
    }
}

impl Running {
    /// Ask the large diff to move its focus; bytes ignore it.
    fn focus(&self, line: u64) {
        match self {
            Self::Large(diff) => diff.focus(line),
            #[cfg(test)]
            Self::Probe(calls) => calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .focus
                .push(line),
            Self::Bytes { .. } => {}
        }
    }

    /// Ask the large diff for a line window; bytes ignore it.
    fn lines(&self, rows: Range<u64>) {
        match self {
            Self::Large(diff) => diff.lines(rows),
            #[cfg(test)]
            Self::Probe(calls) => calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .lines
                .push(rows),
            Self::Bytes { .. } => {}
        }
    }

    /// Release the large diff's acknowledged snapshots; bytes ignore it.
    fn acknowledge(&self, upto: u64) {
        match self {
            Self::Large(diff) => diff.acknowledge(upto),
            #[cfg(test)]
            Self::Probe(calls) => calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .acked
                .push(upto),
            Self::Bytes { .. } => {}
        }
    }
}

/// The large engine's input; its sides are built again on its own workers.
fn large_input(request: &PairRequest, load: &LoadInput, plan: &EnginePlan) -> LargeInput {
    LargeInput {
        left: request.left.clone(),
        right: request.right.clone(),
        settings: plan.settings.clone(),
        sources: Sources::new(spool_path(plan)),
        options: load.options,
        fallback: load.fallback,
        budget: plan.budget,
        focus: 0,
    }
}

/// The spool base, detected on the worker because it stats portable markers.
fn spool_path(plan: &EnginePlan) -> Option<PathBuf> {
    if plan.spool {
        StoreDirs::detect().map(|dirs| dirs.data.join(SPOOL_DIR))
    } else {
        None
    }
}

/// Commands from the screen that must reach the engine, or wait for the next one after a refresh.
#[derive(Default)]
struct LiveState {
    engine: Option<Running>,
    lines: Option<Range<u64>>,
    focus: Option<u64>,
    closed: bool,
}

/// The live engine slot and the last line/focus request, replayed when a refreshed engine starts.
pub(crate) struct Live {
    state: Mutex<LiveState>,
}

impl Live {
    /// An empty slot; requests wait until the worker installs an engine.
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(LiveState::default()),
        }
    }

    /// Install a refreshed engine, replaying the waiting line window and focus.
    pub(crate) fn install(&self, engine: Option<Running>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.closed {
            return;
        }
        state.engine = engine;
        let (lines, focus) = (state.lines.take(), state.focus.take());
        if let Some(engine) = state.engine.as_ref() {
            if let Some(rows) = lines {
                engine.lines(rows);
            }
            if let Some(line) = focus {
                engine.focus(line);
            }
        }
    }

    /// Ask the engine for `rows`, or keep the request for the next engine.
    pub(crate) fn lines(&self, rows: Range<u64>) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        match state.engine.as_ref() {
            Some(engine) => engine.lines(rows),
            None => state.lines = Some(rows),
        }
    }

    /// Move the engine's focus, or keep it for the next engine.
    pub(crate) fn focus(&self, line: u64) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        match state.engine.as_ref() {
            Some(engine) => engine.focus(line),
            None => state.focus = Some(line),
        }
    }

    /// Release the snapshot `upto` on the live large engine.
    pub(crate) fn acknowledge(&self, upto: u64) {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(engine) = state.engine.as_ref() {
            engine.acknowledge(upto);
        }
    }

    /// Drop the engine, cancelling its workers; a late request waits for the next one.
    pub(crate) fn clear(&self) {
        self.install(None);
    }

    /// Stop for good: drop the engine and refuse every later one the worker may still start.
    pub(crate) fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.closed = true;
        state.engine = None;
        (state.lines, state.focus) = (None, None);
    }
}

#[cfg(test)]
mod tests;
