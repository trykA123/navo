use std::time::{Duration, Instant};

use serde::Serialize;

mod document;
mod latencies;
mod memory;
mod ring;
mod summary;
mod threads;

pub use document::{GpuFacts, MonitorFacts, PerfDocument, SystemFacts, perf_json};
pub use latencies::Latencies;
pub use memory::{
    ComponentSummary, MemoryRecord, MemoryRow, MemorySummary, MemoryTrack, memory_text,
};
pub use ring::Ring;
pub use summary::Summary;
pub use threads::{
    THREAD_CAP, ThreadCpu, ThreadReport, ThreadStat, ThreadTime, TopThread, threads_text,
};

pub const RECORD_FOR: Duration = Duration::from_secs(120);
pub const SLOW_FRAME_MS: f32 = 16.7;
pub const VERY_SLOW_FRAME_MS: f32 = 33.0;

const FRAMES: usize = 32_768;
const SECONDS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Latency {
    CommandCenterKey,
    CommandCenterOpen,
    FolderOpen,
    ContextMenu,
    DrawerOpen,
    MenuOpen,
    ScrollPaint,
}

impl Latency {
    pub const ALL: [Self; 7] = [
        Self::CommandCenterKey,
        Self::CommandCenterOpen,
        Self::FolderOpen,
        Self::ContextMenu,
        Self::DrawerOpen,
        Self::MenuOpen,
        Self::ScrollPaint,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    fn capacity(self) -> usize {
        match self {
            Self::ScrollPaint => 8_192,
            Self::CommandCenterKey => 2_048,
            Self::FolderOpen => 1_024,
            Self::CommandCenterOpen | Self::ContextMenu | Self::DrawerOpen | Self::MenuOpen => 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiFlag {
    ContextMenu,
    CommandCenter,
    Drawer,
    Settings,
    Dialog,
    AppMenu,
    Animation,
    Scroll,
}

impl UiFlag {
    const ALL: [Self; 8] = [
        Self::ContextMenu,
        Self::CommandCenter,
        Self::Drawer,
        Self::Settings,
        Self::Dialog,
        Self::AppMenu,
        Self::Animation,
        Self::Scroll,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::ContextMenu => "context_menu",
            Self::CommandCenter => "command_center",
            Self::Drawer => "drawer",
            Self::Settings => "settings",
            Self::Dialog => "dialog",
            Self::AppMenu => "app_menu",
            Self::Animation => "animation",
            Self::Scroll => "scroll",
        }
    }

    fn bit(self) -> u16 {
        1 << self as u16
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiState(u16);

impl UiState {
    #[must_use]
    pub fn with(self, flag: UiFlag) -> Self {
        Self(self.0 | flag.bit())
    }

    #[must_use]
    pub fn when(self, is_set: bool, flag: UiFlag) -> Self {
        if is_set { self.with(flag) } else { self }
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub fn has(self, flag: UiFlag) -> bool {
        self.0 & flag.bit() != 0
    }

    #[must_use]
    pub fn label(self) -> String {
        let names: Vec<&str> = UiFlag::ALL
            .into_iter()
            .filter(|flag| self.has(*flag))
            .map(UiFlag::name)
            .collect();
        if names.is_empty() {
            "plain".to_owned()
        } else {
            names.join("+")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameSample {
    pub window: u8,
    pub at_ms: f32,
    pub interval_ms: Option<f32>,
    pub layout_ms: f32,
    pub paint_ms: f32,
    pub state: UiState,
}

impl FrameSample {
    #[must_use]
    pub fn work_ms(&self) -> f32 {
        self.layout_ms + self.paint_ms
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LatencySample {
    pub kind: Latency,
    pub at_ms: f32,
    pub ms: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ProcessSample {
    pub at_ms: f32,
    #[serde(rename = "cpu_percent_of_one_core")]
    pub cpu_percent: Option<f32>,
    pub working_set_bytes: Option<u64>,
    pub private_bytes: Option<u64>,
    pub threads: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct IndexSample {
    pub at_ms: u32,
    pub entries: Option<u64>,
    pub mode: &'static str,
    pub service: &'static str,
}

#[derive(Debug, Clone)]
pub struct Recording {
    started: Instant,
    pub frames: Ring<FrameSample>,
    pub latencies: Latencies,
    pub process: Ring<ProcessSample>,
    pub index: Ring<IndexSample>,
}

impl Recording {
    #[must_use]
    pub fn new(started: Instant) -> Self {
        Self {
            started,
            frames: Ring::new(FRAMES),
            latencies: Latencies::new(),
            process: Ring::new(SECONDS),
            index: Ring::new(SECONDS),
        }
    }

    #[must_use]
    pub fn started(&self) -> Instant {
        self.started
    }

    #[must_use]
    pub fn elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.started)
    }

    #[must_use]
    pub fn at_ms(&self, at: Instant) -> f32 {
        ms(self.elapsed(at))
    }

    #[must_use]
    pub fn is_due(&self, now: Instant) -> bool {
        self.elapsed(now) >= RECORD_FOR
    }

    pub fn push_latency(&mut self, kind: Latency, start: Instant, end: Instant) {
        let sample = LatencySample {
            kind,
            at_ms: self.at_ms(start),
            ms: ms(end.saturating_duration_since(start)),
        };
        self.latencies.push(sample);
    }
}

#[must_use]
pub fn ms(duration: Duration) -> f32 {
    duration.as_secs_f32() * 1000.0
}

#[cfg(test)]
mod memory_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod thread_tests;
