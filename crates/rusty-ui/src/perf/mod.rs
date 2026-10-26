mod recorder;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use gpui_kit::{DispatchPhase, ScrollWheelEvent, canvas, deferred};
pub use rusty_diag::perf::{
    GpuFacts, Latency, MonitorFacts, ProcessSample, RECORD_FOR, Recording, SystemFacts, UiFlag,
    UiState,
};

use crate::prelude::*;
use recorder::Recorder;

pub trait PerfService: Send + Sync {
    fn start(&self);
    fn sample(&self) -> ProcessSample;
    fn system(&self) -> SystemFacts;
    fn save(
        &self,
        recording: &Recording,
        ended: Instant,
        system: &SystemFacts,
    ) -> Result<PathBuf, String>;
}

static RECORDERS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecordingId(u64);

struct Active {
    id: RecordingId,
    recorder: Recorder,
    gpu: Option<GpuFacts>,
}

thread_local! {
    static RECORDER: RefCell<Option<Active>> = const { RefCell::new(None) };
    static NEXT_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
    static SERVICE: RefCell<Option<Arc<dyn PerfService>>> = const { RefCell::new(None) };
}

pub fn install(service: Arc<dyn PerfService>) {
    SERVICE.with_borrow_mut(|slot| *slot = Some(service));
}

pub(crate) fn service() -> Option<Arc<dyn PerfService>> {
    SERVICE.with_borrow(Clone::clone)
}

#[must_use]
pub fn is_recording() -> bool {
    RECORDERS.load(Ordering::Relaxed) > 0
}

pub(crate) fn start(now: Instant, gpu: Option<GpuFacts>) -> Option<RecordingId> {
    RECORDER.with_borrow_mut(|slot| {
        if slot.is_some() {
            return None;
        }
        let id = RecordingId(NEXT_ID.replace(NEXT_ID.get().wrapping_add(1)));
        *slot = Some(Active {
            id,
            recorder: Recorder::new(now),
            gpu,
        });
        RECORDERS.fetch_add(1, Ordering::Relaxed);
        Some(id)
    })
}

pub(crate) fn current() -> Option<RecordingId> {
    RECORDER.with_borrow(|slot| slot.as_ref().map(|active| active.id))
}

pub(crate) fn stop() -> Option<(Recording, Option<GpuFacts>)> {
    let active = RECORDER.with_borrow_mut(Option::take)?;
    RECORDERS.fetch_sub(1, Ordering::Relaxed);
    Some((active.recorder.into_recording(), active.gpu))
}

fn with(update: impl FnOnce(&mut Recorder)) {
    if !is_recording() {
        return;
    }
    RECORDER.with_borrow_mut(|slot| {
        if let Some(active) = slot {
            update(&mut active.recorder);
        }
    });
}

#[cfg(test)]
pub(crate) fn elapsed(now: Instant) -> Option<std::time::Duration> {
    let mut elapsed = None;
    with(|recorder| elapsed = Some(recorder.recording.elapsed(now)));
    elapsed
}

pub(crate) fn record(update: impl FnOnce(&mut Recording, Instant)) {
    with(|recorder| update(&mut recorder.recording, Instant::now()));
}

pub fn begin(kind: Latency) {
    with(|recorder| recorder.begin(kind, Instant::now()));
}

pub fn ready(kind: Latency) {
    with(|recorder| recorder.ready(kind, Instant::now()));
}

pub fn cancel(kind: Latency) {
    with(|recorder| recorder.cancel(kind));
}

pub fn animating(kind: Latency, is_running: bool) {
    with(|recorder| recorder.animating(kind, is_running, Instant::now()));
}

pub fn note(flag: UiFlag) {
    with(|recorder| recorder.note(flag));
}

fn note_scroll() {
    with(|recorder| recorder.note_scroll(Instant::now()));
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameStart {
    window: u64,
    at: Instant,
    state: UiState,
}

pub(crate) fn frame_begin(read: impl FnOnce() -> (u64, UiState)) -> Option<FrameStart> {
    let mut started = None;
    with(|recorder| {
        let (window, state) = read();
        recorder.clear_frame(window);
        started = Some(FrameStart {
            window,
            at: Instant::now(),
            state,
        });
    });
    started
}

pub(crate) fn frame_end(start: FrameStart) -> impl IntoElement {
    let probe = canvas(
        |_, _, _| Instant::now(),
        move |_, laid_out, window, _| {
            window.on_mouse_event(|_: &ScrollWheelEvent, phase, _, _| {
                if phase == DispatchPhase::Capture {
                    note_scroll();
                }
            });
            let painted = Instant::now();
            with(|recorder| recorder.finish_frame(start, laid_out, painted));
        },
    );
    deferred(probe.absolute().size_0()).priority(usize::MAX)
}
