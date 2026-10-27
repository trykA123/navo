use std::collections::BTreeMap;

use serde::Serialize;

use super::{
    FrameSample, IndexSample, Latency, LatencySample, ProcessSample, Recording, SLOW_FRAME_MS,
    Summary, ThreadReport, TopThread, VERY_SLOW_FRAME_MS, ms,
};
use crate::Redactor;

const FORMAT: u32 = 1;

#[derive(Debug, Clone, Default, Serialize)]
pub struct GpuFacts {
    pub name: String,
    pub driver: String,
    pub driver_info: String,
    pub backend: String,
    pub is_software: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct MonitorFacts {
    pub width_px: u32,
    pub height_px: u32,
    pub refresh_hz: Option<f32>,
    pub scale: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SystemFacts {
    pub os: String,
    pub cpu_model: String,
    pub logical_cores: usize,
    pub ram_bytes: u64,
    pub gpu: Option<GpuFacts>,
    pub monitors: Vec<MonitorFacts>,
}

impl SystemFacts {
    fn redacted(&self, redactor: &Redactor) -> Self {
        let gpu = self.gpu.as_ref().map(|gpu| GpuFacts {
            name: redactor.text(&gpu.name),
            driver: redactor.text(&gpu.driver),
            driver_info: redactor.text(&gpu.driver_info),
            backend: redactor.text(&gpu.backend),
            is_software: gpu.is_software,
        });
        Self {
            os: redactor.text(&self.os),
            cpu_model: redactor.text(&self.cpu_model),
            gpu,
            ..self.clone()
        }
    }
}

#[derive(Debug, Default, Serialize)]
struct SlowCount {
    frames: u64,
    over_16_7_ms: u64,
    over_33_ms: u64,
}

impl SlowCount {
    fn add(&mut self, frame: &FrameSample) {
        let work = frame.work_ms();
        self.frames += 1;
        self.over_16_7_ms += u64::from(work > SLOW_FRAME_MS);
        self.over_33_ms += u64::from(work > VERY_SLOW_FRAME_MS);
    }
}

#[derive(Debug, Serialize)]
struct Summaries {
    frame_work_ms: Option<Summary>,
    layout_ms: Option<Summary>,
    paint_ms: Option<Summary>,
    interval_ms_while_moving: Option<Summary>,
    slow_frames: SlowCount,
    slow_frames_by_state: BTreeMap<String, SlowCount>,
    latency_ms: BTreeMap<Latency, Summary>,
    #[serde(rename = "cpu_percent_of_one_core")]
    cpu_percent: Option<Summary>,
    working_set_bytes: Option<Summary>,
    private_bytes: Option<Summary>,
    threads_max: Option<u32>,
    last_index: Option<IndexSample>,
    top_threads: Vec<TopThread>,
}

#[derive(Debug, Serialize)]
struct Dropped {
    frames: u64,
    latencies: u64,
    process: u64,
    index: u64,
}

#[derive(Debug, Serialize)]
struct RawFrame {
    window: u8,
    at_ms: f64,
    interval_ms: Option<f64>,
    layout_ms: f64,
    paint_ms: f64,
    state: String,
}

#[derive(Debug, Serialize)]
struct Samples {
    frames: Vec<RawFrame>,
    latencies: Vec<LatencySample>,
    process: Vec<ProcessSample>,
    index: Vec<IndexSample>,
}

#[derive(Debug, Serialize)]
pub struct PerfDocument {
    format: u32,
    recorded_ms: f64,
    system: SystemFacts,
    summary: Summaries,
    dropped: Dropped,
    threads: ThreadReport,
    samples: Samples,
}

impl PerfDocument {
    #[must_use]
    pub fn new(
        recording: &Recording,
        ended: std::time::Instant,
        system: &SystemFacts,
        redactor: &Redactor,
    ) -> Self {
        Self {
            format: FORMAT,
            recorded_ms: round(ms(recording.elapsed(ended))),
            system: system.redacted(redactor),
            summary: summaries(recording),
            dropped: Dropped {
                frames: recording.frames.dropped(),
                latencies: recording.latencies.dropped(),
                process: recording.process.dropped(),
                index: recording.index.dropped(),
            },
            threads: ThreadReport::default(),
            samples: samples(recording),
        }
    }
}

impl PerfDocument {
    #[must_use]
    pub fn with_threads(mut self, threads: &ThreadReport, redactor: &Redactor) -> Self {
        self.threads = threads.clone().redacted(redactor);
        self.summary.top_threads = self.threads.top();
        self
    }
}

pub fn perf_json(document: &PerfDocument) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec_pretty(document)
}

fn summaries(recording: &Recording) -> Summaries {
    let frames = || recording.frames.iter();
    let process = || recording.process.iter();
    let mut slow_frames = SlowCount::default();
    let mut by_state: BTreeMap<String, SlowCount> = BTreeMap::new();
    for frame in frames() {
        slow_frames.add(frame);
        by_state.entry(frame.state.label()).or_default().add(frame);
    }
    let latency_ms = Latency::ALL
        .into_iter()
        .filter_map(|kind| {
            let values = recording.latencies.of(kind).iter();
            Some((kind, Summary::of(values.map(|s| f64::from(s.ms)))?))
        })
        .collect();
    Summaries {
        frame_work_ms: Summary::of(frames().map(|f| f64::from(f.work_ms()))),
        layout_ms: Summary::of(frames().map(|f| f64::from(f.layout_ms))),
        paint_ms: Summary::of(frames().map(|f| f64::from(f.paint_ms))),
        interval_ms_while_moving: Summary::of(
            frames().filter_map(|f| f.interval_ms.map(f64::from)),
        ),
        slow_frames,
        slow_frames_by_state: by_state,
        latency_ms,
        cpu_percent: Summary::of(process().filter_map(|p| p.cpu_percent.map(f64::from))),
        working_set_bytes: Summary::of(process().filter_map(|p| p.working_set_bytes.map(wide))),
        private_bytes: Summary::of(process().filter_map(|p| p.private_bytes.map(wide))),
        threads_max: process().filter_map(|p| p.threads).max(),
        last_index: recording.index.iter().last().copied(),
        top_threads: Vec::new(),
    }
}

fn samples(recording: &Recording) -> Samples {
    let frames = recording
        .frames
        .iter()
        .map(|frame| RawFrame {
            window: frame.window,
            at_ms: round(frame.at_ms),
            interval_ms: frame.interval_ms.map(round),
            layout_ms: round(frame.layout_ms),
            paint_ms: round(frame.paint_ms),
            state: frame.state.label(),
        })
        .collect();
    let mut latencies: Vec<LatencySample> = recording.latencies.iter().copied().collect();
    latencies.sort_by(|a, b| a.at_ms.total_cmp(&b.at_ms));
    Samples {
        frames,
        latencies,
        process: recording.process.iter().copied().collect(),
        index: recording.index.iter().copied().collect(),
    }
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 1000.0).round() / 1000.0
}

fn wide(bytes: u64) -> f64 {
    bytes as f64
}
