use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::{SECONDS, Summary};
use crate::Redactor;

pub const THREAD_CAP: usize = 512;
const TOP: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadTime {
    pub id: u32,
    pub name: Option<String>,
    pub cpu: Duration,
}

#[derive(Debug, Clone)]
struct Tracked {
    name: Option<String>,
    last: Duration,
    seen: u64,
    total: Duration,
    percents: Vec<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadCpu {
    tracked: BTreeMap<u32, Tracked>,
    last_at: Option<Instant>,
    tick: u64,
    ignored_max: usize,
}

impl ThreadCpu {
    pub fn record(&mut self, at: Instant, threads: &[ThreadTime]) {
        let previous = self.last_at.replace(at);
        self.tick += 1;
        let wall = previous.map(|before| at.saturating_duration_since(before));
        let mut ignored = 0;
        for thread in threads {
            if !self.add(thread, wall) {
                ignored += 1;
            }
        }
        self.ignored_max = self.ignored_max.max(ignored);
    }

    fn add(&mut self, thread: &ThreadTime, wall: Option<Duration>) -> bool {
        let tick = self.tick;
        if !self.tracked.contains_key(&thread.id) && self.tracked.len() >= THREAD_CAP {
            return false;
        }
        let tracked = self.tracked.entry(thread.id).or_insert_with(|| Tracked {
            name: None,
            last: Duration::ZERO,
            seen: 0,
            total: Duration::ZERO,
            percents: Vec::new(),
        });
        let continued = tracked.seen + 1 == tick && thread.cpu >= tracked.last;
        let spent = if continued {
            thread.cpu - tracked.last
        } else {
            thread.cpu
        };
        if thread.name.is_some() {
            tracked.name.clone_from(&thread.name);
        }
        tracked.last = thread.cpu;
        tracked.seen = tick;
        let Some(wall) = wall.filter(|wall| !wall.is_zero()) else {
            return true;
        };
        tracked.total += spent;
        if tracked.percents.len() < SECONDS {
            tracked.percents.push(percent(spent, wall));
        }
        true
    }

    #[must_use]
    pub fn report(&self) -> ThreadReport {
        let threads = self
            .tracked
            .iter()
            .map(|(id, tracked)| (*id, tracked.stat(*id)))
            .collect();
        ThreadReport {
            cap: THREAD_CAP,
            ignored_threads_max: self.ignored_max,
            threads,
        }
    }
}

impl Tracked {
    fn stat(&self, id: u32) -> ThreadStat {
        let summary = Summary::of(self.percents.iter().copied().map(f64::from));
        ThreadStat {
            name: self.name.clone().unwrap_or_else(|| format!("thread {id}")),
            total_cpu_ms: round(self.total.as_secs_f64() * 1000.0),
            p50_percent_of_one_core: summary.map_or(0.0, |summary| round(summary.p50)),
            max_percent_of_one_core: summary.map_or(0.0, |summary| round(summary.max)),
            intervals: self.percents.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThreadStat {
    pub name: String,
    pub total_cpu_ms: f64,
    pub p50_percent_of_one_core: f64,
    pub max_percent_of_one_core: f64,
    pub intervals: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopThread {
    pub id: u32,
    pub name: String,
    pub total_cpu_ms: f64,
    pub peak_percent_of_one_core: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThreadReport {
    pub cap: usize,
    pub ignored_threads_max: usize,
    #[serde(rename = "by_id")]
    pub threads: BTreeMap<u32, ThreadStat>,
}

impl Default for ThreadReport {
    fn default() -> Self {
        ThreadCpu::default().report()
    }
}

impl ThreadReport {
    #[must_use]
    pub fn top(&self) -> Vec<TopThread> {
        let mut top: Vec<TopThread> = self
            .threads
            .iter()
            .map(|(id, stat)| TopThread {
                id: *id,
                name: stat.name.clone(),
                total_cpu_ms: stat.total_cpu_ms,
                peak_percent_of_one_core: stat.max_percent_of_one_core,
            })
            .collect();
        top.sort_by(|a, b| {
            b.total_cpu_ms
                .total_cmp(&a.total_cpu_ms)
                .then(a.id.cmp(&b.id))
        });
        top.truncate(TOP);
        top
    }

    #[must_use]
    pub(super) fn redacted(mut self, redactor: &Redactor) -> Self {
        for stat in self.threads.values_mut() {
            stat.name = redactor.text(&stat.name);
        }
        self
    }
}

#[must_use]
pub fn threads_text(report: &ThreadReport, redactor: &Redactor) -> String {
    let report = report.clone().redacted(redactor);
    let mut text = String::from("Busiest threads (CPU while recording)\n");
    for thread in report.top() {
        let _ = writeln!(
            text,
            "{} ({}): {:.0} ms total, peak {:.1} %",
            thread.name, thread.id, thread.total_cpu_ms, thread.peak_percent_of_one_core
        );
    }
    if report.threads.is_empty() {
        text.push_str("No thread samples on this system.\n");
    }
    if report.ignored_threads_max > 0 {
        let _ = writeln!(
            text,
            "Up to {} threads were not tracked (limit {}).",
            report.ignored_threads_max, report.cap
        );
    }
    text
}

fn percent(spent: Duration, wall: Duration) -> f32 {
    (spent.as_secs_f64() / wall.as_secs_f64() * 100.0) as f32
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
