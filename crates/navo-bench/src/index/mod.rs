mod memory;

use serde::{Deserialize, Serialize};

pub use memory::{IndexMemory, MemoryPartKind, PartMemory, VolumeMemory, parse_memory};

/// Why a saved sample could not be read. Carries a role only, never input text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    Json,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("index-sample:unreadable")
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceState {
    Running,
    NotRunning,
    NotInstalled,
    Unknown,
}

/// One fixed query, named by its position in the check (0-based), never by its text.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct QueryTime {
    pub id: u8,
    pub runs: u32,
    pub min_micros: u64,
    pub median_micros: u64,
    pub max_micros: u64,
    pub results: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IndexReport {
    pub service: ServiceState,
    pub cold_start_micros: Option<u64>,
    pub warm_start_micros: Option<u64>,
    pub first_answer_micros: Option<u64>,
    pub first_building_answer_micros: Option<u64>,
    pub entries: u64,
    pub building: bool,
    pub queries: Vec<QueryTime>,
    pub memory: IndexMemory,
}

impl IndexReport {
    /// The check could not run; every number is absent.
    pub fn unavailable(service: ServiceState) -> Self {
        Self {
            service,
            cold_start_micros: None,
            warm_start_micros: None,
            first_answer_micros: None,
            first_building_answer_micros: None,
            entries: 0,
            building: false,
            queries: Vec::new(),
            memory: IndexMemory::default(),
        }
    }

    /// Total of the service's private bytes, when the sample holds it.
    pub fn memory_bytes(&self) -> Option<u64> {
        self.memory.process_bytes()
    }
}

#[derive(Deserialize)]
struct RawIndex {
    cold_start_ms: Option<f64>,
    warm_start_ms: Option<f64>,
    first_answer_ms: Option<f64>,
    first_building_answer_ms: Option<f64>,
    #[serde(default)]
    entries: u64,
    #[serde(default)]
    building: bool,
    #[serde(default)]
    queries: Vec<RawQuery>,
    memory: Option<memory::RawMemory>,
}

#[derive(Deserialize)]
struct RawQuery {
    #[serde(default)]
    runs: u32,
    #[serde(default)]
    min_ms: f64,
    #[serde(default)]
    median_ms: f64,
    #[serde(default)]
    max_ms: f64,
    #[serde(default)]
    results: u64,
}

pub(crate) fn micros(ms: f64) -> u64 {
    if ms.is_finite() && ms > 0.0 {
        (ms * 1000.0).round() as u64
    } else {
        0
    }
}

/// Reads the JSON of `navo --bench index --json`; query text in it is dropped.
pub fn parse_index(json: &str) -> Result<IndexReport, ParseError> {
    let raw: RawIndex = serde_json::from_str(json).map_err(|_| ParseError::Json)?;
    let queries = raw
        .queries
        .iter()
        .enumerate()
        .map(|(position, query)| QueryTime {
            id: u8::try_from(position).unwrap_or(u8::MAX),
            runs: query.runs,
            min_micros: micros(query.min_ms),
            median_micros: micros(query.median_ms),
            max_micros: micros(query.max_ms),
            results: query.results,
        })
        .collect();
    Ok(IndexReport {
        service: ServiceState::Running,
        cold_start_micros: raw.cold_start_ms.map(micros),
        warm_start_micros: raw.warm_start_ms.map(micros),
        first_answer_micros: raw.first_answer_ms.map(micros),
        first_building_answer_micros: raw.first_building_answer_ms.map(micros),
        entries: raw.entries,
        building: raw.building,
        queries,
        memory: raw
            .memory
            .map(memory::RawMemory::into_memory)
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests;
