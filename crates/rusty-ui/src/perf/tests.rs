use std::time::{Duration, Instant};

use super::recorder::Recorder;
use super::{FrameStart, Latency, UiFlag, UiState};

const MS: Duration = Duration::from_millis(1);
const WINDOW: u64 = 42;

fn start(at: Instant, state: UiState) -> FrameStart {
    FrameStart {
        window: WINDOW,
        at,
        state,
    }
}

fn frame(recorder: &mut Recorder, at: Instant, state: UiState) {
    recorder.finish_frame(start(at, state), at + 2 * MS, at + 3 * MS);
}

fn latencies(recorder: Recorder) -> Vec<(Latency, f32)> {
    recorder
        .into_recording()
        .latencies
        .iter()
        .map(|sample| (sample.kind, sample.ms.round()))
        .collect()
}

#[test]
fn hooks_do_nothing_while_not_recording() {
    assert!(super::frame_begin(|| (1, UiState::default())).is_none());
    super::begin(Latency::FolderOpen);
    assert!(super::stop().is_none());
}

#[test]
fn a_frame_records_layout_and_paint_time() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    frame(
        &mut recorder,
        now,
        UiState::default().with(UiFlag::Settings),
    );
    let recording = recorder.into_recording();
    let sample = recording.frames.iter().next().copied().unwrap();
    assert_eq!(
        (sample.layout_ms.round(), sample.paint_ms.round()),
        (2.0, 1.0)
    );
    assert!(sample.state.has(UiFlag::Settings));
}

#[test]
fn a_latency_ends_at_the_paint_after_it_is_ready() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.begin(Latency::CommandCenterKey, now);
    frame(&mut recorder, now + 5 * MS, UiState::default());
    recorder.ready(Latency::CommandCenterKey, now + 6 * MS);
    frame(&mut recorder, now + 20 * MS, UiState::default());
    assert_eq!(latencies(recorder), [(Latency::CommandCenterKey, 23.0)]);
}

#[test]
fn a_second_keystroke_before_the_paint_keeps_the_first_start() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.begin(Latency::CommandCenterKey, now);
    recorder.begin(Latency::CommandCenterKey, now + 8 * MS);
    recorder.ready(Latency::CommandCenterKey, now + 9 * MS);
    frame(&mut recorder, now + 10 * MS, UiState::default());
    assert_eq!(latencies(recorder), [(Latency::CommandCenterKey, 13.0)]);
}

#[test]
fn ready_without_a_start_records_nothing() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.ready(Latency::ContextMenu, now);
    frame(&mut recorder, now, UiState::default());
    assert!(latencies(recorder).is_empty());
}

#[test]
fn an_animation_runs_from_its_first_frame_until_it_settles() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.animating(Latency::DrawerOpen, true, now);
    recorder.animating(Latency::DrawerOpen, true, now + 100 * MS);
    recorder.animating(Latency::DrawerOpen, false, now + 180 * MS);
    recorder.animating(Latency::DrawerOpen, false, now + 300 * MS);
    assert_eq!(latencies(recorder), [(Latency::DrawerOpen, 180.0)]);
}

#[test]
fn a_running_animation_marks_the_frame() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.clear_frame(WINDOW);
    recorder.animating(Latency::MenuOpen, true, now);
    frame(&mut recorder, now, UiState::default());
    frame(&mut recorder, now + 17 * MS, UiState::default());
    let recording = recorder.into_recording();
    let states: Vec<bool> = recording
        .frames
        .iter()
        .map(|frame| frame.state.has(UiFlag::Animation))
        .collect();
    assert_eq!(states, [true, false]);
}

#[test]
fn intervals_count_only_after_a_moving_frame() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    let moving = UiState::default().with(UiFlag::Animation);
    frame(&mut recorder, now, UiState::default());
    frame(&mut recorder, now + 900 * MS, moving);
    frame(&mut recorder, now + 920 * MS, UiState::default());
    let intervals: Vec<Option<f32>> = recorder
        .into_recording()
        .frames
        .iter()
        .map(|frame| frame.interval_ms.map(f32::round))
        .collect();
    assert_eq!(intervals, [None, None, Some(20.0)]);
}

#[test]
fn a_scroll_is_measured_to_the_next_paint_and_marks_the_frame() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.note_scroll(now);
    frame(&mut recorder, now + 4 * MS, UiState::default());
    let recording = recorder.into_recording();
    assert!(
        recording
            .frames
            .iter()
            .all(|frame| frame.state.has(UiFlag::Scroll))
    );
    let scroll: Vec<f32> = recording.latencies.iter().map(|s| s.ms.round()).collect();
    assert_eq!(scroll, [7.0]);
}

#[test]
fn a_drawer_closed_mid_animation_leaves_no_stale_start() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.clear_frame(WINDOW);
    recorder.animating(Latency::DrawerOpen, true, now);
    frame(&mut recorder, now, UiState::default());
    recorder.clear_frame(WINDOW);
    frame(&mut recorder, now + 17 * MS, UiState::default());
    let reopened = now + 10_000 * MS;
    recorder.clear_frame(WINDOW);
    recorder.animating(Latency::DrawerOpen, true, reopened);
    frame(&mut recorder, reopened, UiState::default());
    recorder.clear_frame(WINDOW);
    recorder.animating(Latency::DrawerOpen, false, reopened + 180 * MS);
    frame(&mut recorder, reopened + 180 * MS, UiState::default());
    assert_eq!(latencies(recorder), [(Latency::DrawerOpen, 180.0)]);
}

#[test]
fn another_window_frame_keeps_a_running_animation() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.clear_frame(WINDOW);
    recorder.animating(Latency::MenuOpen, true, now);
    frame(&mut recorder, now, UiState::default());
    recorder.clear_frame(WINDOW + 1);
    let other = FrameStart {
        window: WINDOW + 1,
        at: now + 5 * MS,
        state: UiState::default(),
    };
    recorder.finish_frame(other, now + 6 * MS, now + 7 * MS);
    recorder.clear_frame(WINDOW);
    recorder.animating(Latency::MenuOpen, false, now + 100 * MS);
    assert_eq!(latencies(recorder), [(Latency::MenuOpen, 100.0)]);
}

#[test]
fn an_armed_start_with_no_ready_expires() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.begin(Latency::ContextMenu, now);
    frame(&mut recorder, now + 6_000 * MS, UiState::default());
    recorder.ready(Latency::ContextMenu, now + 7_000 * MS);
    frame(&mut recorder, now + 7_010 * MS, UiState::default());
    recorder.begin(Latency::FolderOpen, now);
    recorder.ready(Latency::FolderOpen, now + 6_000 * MS);
    frame(&mut recorder, now + 6_010 * MS, UiState::default());
    assert!(latencies(recorder).is_empty());
}

#[test]
fn a_stale_armed_start_is_replaced_by_a_new_begin() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.begin(Latency::FolderOpen, now);
    let later = now + 60_000 * MS;
    recorder.begin(Latency::FolderOpen, later);
    recorder.ready(Latency::FolderOpen, later + 10 * MS);
    frame(&mut recorder, later + 10 * MS, UiState::default());
    assert_eq!(latencies(recorder), [(Latency::FolderOpen, 13.0)]);
}

#[test]
fn a_cancelled_start_records_nothing() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    recorder.begin(Latency::ContextMenu, now);
    recorder.cancel(Latency::ContextMenu);
    recorder.ready(Latency::ContextMenu, now + 5 * MS);
    frame(&mut recorder, now + 5 * MS, UiState::default());
    assert!(latencies(recorder).is_empty());
}

fn reserved(recorder: &Recorder) -> Vec<usize> {
    let recording = &recorder.recording;
    let mut reserved = vec![recording.frames.allocated()];
    reserved.extend(Latency::ALL.map(|kind| recording.latencies.of(kind).allocated()));
    reserved
}

#[test]
fn the_frame_hooks_never_grow_the_recording_storage() {
    let now = Instant::now();
    let mut recorder = Recorder::new(now);
    let before = reserved(&recorder);
    for index in 0..20_000_u32 {
        let at = now + MS * index;
        recorder.clear_frame(WINDOW);
        recorder.note(UiFlag::Drawer);
        recorder.animating(Latency::DrawerOpen, index % 50 != 0, at);
        recorder.note_scroll(at);
        recorder.begin(Latency::CommandCenterKey, at);
        recorder.ready(Latency::CommandCenterKey, at);
        recorder.finish_frame(start(at, UiState::default()), at, at);
    }
    assert_eq!(reserved(&recorder), before);
    assert_eq!(recorder.recording.frames.len(), 20_000);
}
