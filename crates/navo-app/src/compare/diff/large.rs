//! Large text diffs on workers (V35, D16): one thread drains events, one reads display lines.
//!
//! The engine gets its own sources; the display reader opens separate instances (rule 6).

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use navo_compare::model::{Budget, CancelToken, Reason};
use navo_compare::text::Options;
use navo_compare::text::large::{LargeDiff as Engine, LargeSide, Request};
use navo_compare::text::side::TextSide;
use navo_core::Location;
use navo_core::paging::TextLayout;
use navo_ui::compare_diff::{DiffSink, DiffUpdate, PairControl, PairRequest};

use super::super::settings::CompareSettings;
use super::sides::side_of;
use super::sources::{Sources, empty_source};

mod acks;
pub(crate) mod convert;
pub(crate) mod lines;
pub(crate) mod queue;
pub(crate) mod state;

use lines::Reader;
use queue::{Command, Commands, LineQueue};
use state::State;

const POLL: Duration = Duration::from_millis(20);

/// The pair id and sink shared by the workers.
pub(crate) struct Ctx {
    pub(crate) id: u64,
    pub(crate) sink: DiffSink,
}

#[cfg(test)]
pub(crate) use state::State as TestState;

/// Everything a large diff needs to build its two sides on the workers.
#[derive(Clone)]
pub(crate) struct LargeInput {
    pub left: Option<Location>,
    pub right: Option<Location>,
    pub settings: CompareSettings,
    pub sources: Sources,
    pub options: Options,
    pub fallback: &'static encoding_rs::Encoding,
    pub budget: Budget,
    pub focus: u64,
}

/// An open large text diff. Commands only queue; dropping cancels off the caller.
pub(crate) struct LargeDiff {
    commands: Arc<Commands>,
    lines: Arc<LineQueue>,
    worker: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
}

impl LargeDiff {
    /// Start the drain and line workers with a pair request.
    pub(crate) fn start(request: PairRequest, input: LargeInput, sink: DiffSink) -> Self {
        let commands = Arc::new(Commands::new());
        let lines = Arc::new(LineQueue::new(commands.cancel.clone()));
        let (id, reader_input) = (request.id, input.clone());
        let (main_commands, main_sink) = (Arc::clone(&commands), sink.clone());
        let worker = spawn("compare-large", move || {
            run(id, input, main_sink, &main_commands)
        });
        let reader_lines = Arc::clone(&lines);
        let reader = spawn("compare-large-lines", move || {
            run_lines(id, reader_input, sink, &reader_lines)
        });
        Self {
            commands,
            lines,
            worker,
            reader,
        }
    }
}

impl PairControl for LargeDiff {
    fn spans(&self, _rows: Range<u32>) {}

    fn refresh(&self, _request: u64) {}

    /// Move the diff's focus; the worker applies it only after `Done`.
    fn focus(&self, line: u64) {
        self.commands.focus(line);
    }

    /// Ask the line worker for a window; a newer request replaces the waiting one.
    fn lines(&self, rows: Range<u64>) {
        self.lines.push(rows);
    }

    /// Release eviction charges after the view applied snapshot number `upto`.
    fn acknowledge(&self, upto: u64) {
        self.commands.acknowledge(upto);
    }
}

impl Drop for LargeDiff {
    /// Cancel and join both workers on a reaper thread, never on the caller.
    fn drop(&mut self) {
        self.commands.close();
        self.lines.close();
        let handles = [self.worker.take(), self.reader.take()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let joined = thread::Builder::new()
            .name("compare-large-join".to_owned())
            .spawn(move || join_all(handles));
        if let Err(error) = joined {
            tracing::warn!(%error, "compare large join thread did not start");
        }
    }
}

/// Join every worker, logging a panic without failing the whole reaper.
fn join_all(handles: Vec<JoinHandle<()>>) {
    for handle in handles {
        if handle.join().is_err() {
            tracing::warn!("compare large worker panicked");
        }
    }
}

/// Spawn a named worker, logging when the OS refuses.
fn spawn(name: &str, run: impl FnOnce() + Send + 'static) -> Option<JoinHandle<()>> {
    match thread::Builder::new().name(name.to_owned()).spawn(run) {
        Ok(handle) => Some(handle),
        Err(error) => {
            tracing::warn!(%error, %name, "compare large worker did not start");
            None
        }
    }
}

/// Build both engine sides, then drain events and commands until closed.
fn run(id: u64, input: LargeInput, sink: DiffSink, commands: &Commands) {
    let (left, right) = match build(&input, &commands.cancel) {
        Ok(sides) => sides,
        Err(reason) => {
            if reason != Reason::Stopped {
                sink(DiffUpdate::Failed {
                    request: id,
                    text: failure_text(reason),
                });
            }
            return;
        }
    };
    let ctx = Ctx { id, sink };
    let mut state = State::new();
    let request = Request {
        left,
        right,
        options: input.options,
        focus: input.focus,
        budget: input.budget,
    };
    let engine = Engine::start(request, &commands.cancel);
    loop {
        if commands.closed.load(Ordering::Acquire) || commands.cancel.is_cancelled() {
            break;
        }
        drain_commands(&mut state, &engine, commands, &ctx);
        match engine.events().recv_timeout(POLL) {
            Ok(event) => {
                if let Some(line) = convert::apply(event, &mut state, &ctx) {
                    convert::focus(line, &mut state, &engine, &ctx);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Apply every queued command now.
fn drain_commands(state: &mut State, engine: &Engine, commands: &Commands, ctx: &Ctx) {
    while let Some(command) = commands.next() {
        match command {
            Command::Focus(line) if state.done => convert::focus(line, state, engine, ctx),
            Command::Focus(line) => state.pending_focus = Some(line),
            Command::Ack(upto) => {
                let freed = state.acks.acknowledge(upto);
                if let Some(seq) = freed.seq {
                    engine.acknowledge_evictions(seq);
                }
                for release in freed.releases {
                    release();
                }
            }
        }
    }
}

/// Build both engine sides from their locations.
fn build(input: &LargeInput, cancel: &CancelToken) -> Result<(LargeSide, LargeSide), Reason> {
    let left = build_side(input.left.as_ref(), input, cancel)?;
    let right = build_side(input.right.as_ref(), input, cancel)?;
    Ok((left, right))
}

/// Decode one side for its layout and open a fresh positional source.
fn build_side(
    location: Option<&Location>,
    input: &LargeInput,
    cancel: &CancelToken,
) -> Result<LargeSide, Reason> {
    let Some(location) = location else {
        return Ok(LargeSide {
            source: empty_source(),
            layout: TextLayout::default(),
        });
    };
    let (side, entry) = side_of(location, &input.settings)?;
    let text = TextSide::decode(side.as_ref(), &entry, input.fallback)?;
    let layout = match text {
        TextSide::Whole(whole) => whole.layout,
        TextSide::Large(large) => large.layout,
    };
    let source = input.sources.open(location, &input.settings, cancel)?;
    if cancel.is_cancelled() {
        return Err(Reason::Stopped);
    }
    Ok(LargeSide { source, layout })
}

/// Build the display reader on its own sources, never the engine's instances.
fn build_reader(input: &LargeInput, cancel: &CancelToken) -> Result<Reader, Reason> {
    let left = build_side(input.left.as_ref(), input, cancel)?;
    let right = build_side(input.right.as_ref(), input, cancel)?;
    Ok(Reader::new(left, right))
}

/// Read requested windows until closed, without touching the event drain.
fn run_lines(id: u64, input: LargeInput, sink: DiffSink, queue: &LineQueue) {
    let cancel = queue.cancel.clone();
    let mut reader: Option<Reader> = None;
    while let Some(rows) = queue.next() {
        if reader.is_none() {
            match build_reader(&input, &cancel) {
                Ok(built) => reader = Some(built),
                Err(Reason::Stopped) => return,
                Err(reason) => {
                    tracing::warn!(?reason, "compare large line sources failed");
                    sink(line_failure(id, reason));
                    return;
                }
            }
        }
        let Some(reader) = reader.as_mut() else {
            return;
        };
        match reader.read(rows, &cancel) {
            Ok(window) => {
                sink(DiffUpdate::LargeLines {
                    request: id,
                    rows: window.rows,
                    left: window.left,
                    right: window.right,
                    left_cut: window.left_cut,
                    right_cut: window.right_cut,
                    totals: window.totals,
                });
            }
            Err(Reason::Stopped) => return,
            Err(reason) => {
                tracing::warn!(?reason, "compare large line read failed");
                sink(line_failure(id, reason));
            }
        }
    }
}

/// The update for a failed line window: `ChangedOnDisk` for a changed source, else `Failed`.
fn line_failure(id: u64, reason: Reason) -> DiffUpdate {
    match reason {
        Reason::ChangedOnDisk => DiffUpdate::ChangedOnDisk { request: id },
        other => DiffUpdate::Failed {
            request: id,
            text: failure_text(other),
        },
    }
}

/// The translated reason for a failed large diff.
fn failure_text(reason: Reason) -> String {
    navo_l10n::tr(reason.fluent_id().as_str())
}
