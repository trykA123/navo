#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MapFlavor {
    Tasking,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRegion {
    pub name: String,
    pub origin: Option<u64>,
    pub length: u64,
    pub used: u64,
    pub free: u64,
    pub reserved: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputSection {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub region: Option<String>,
    pub address_space: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contribution {
    pub object_file: String,
    pub section: String,
    pub size: u64,
    pub address: Option<u64>,
    pub region: Option<String>,
    pub output_section: String,
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub address: u64,
    pub size: Option<u64>,
    pub section: Option<String>,
    pub address_space: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscardedSection {
    pub name: String,
    pub object_file: Option<String>,
    pub library: Option<String>,
    pub symbol: Option<String>,
    pub reason: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapProblem {
    Malformed { line: u64, reason: &'static str },
    CapReached { line: u64, limit: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapModel {
    pub flavor: MapFlavor,
    pub regions: Vec<MemoryRegion>,
    pub sections: Vec<OutputSection>,
    pub contributions: Vec<Contribution>,
    pub symbols: Vec<Symbol>,
    pub discarded: Vec<DiscardedSection>,
    pub problems: Vec<MapProblem>,
    pub problem_total: u64,
    pub skipped_parts: Vec<String>,
    pub lines: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum MapError {
    #[error("reading linker map: {0}")]
    Read(#[from] std::io::Error),
    #[error("indexing linker map was cancelled")]
    Cancelled,
    #[error("the file is not a supported linker map")]
    UnsupportedFormat,
}

impl MapModel {
    pub(super) fn new() -> Self {
        Self {
            flavor: MapFlavor::Tasking,
            regions: Vec::new(),
            sections: Vec::new(),
            contributions: Vec::new(),
            symbols: Vec::new(),
            discarded: Vec::new(),
            problems: Vec::new(),
            problem_total: 0,
            skipped_parts: Vec::new(),
            lines: 0,
        }
    }

    #[must_use]
    pub fn has_symbol_sizes(&self) -> bool {
        self.symbols.iter().any(|symbol| symbol.size.is_some())
    }

    pub(super) fn note(&mut self, problem: MapProblem) {
        self.problem_total += 1;
        if self.problems.len() < super::MAX_MAP_PROBLEMS {
            self.problems.push(problem);
        } else if self.problems.len() == super::MAX_MAP_PROBLEMS {
            self.problems.push(MapProblem::CapReached {
                line: self.lines,
                limit: "retained problems",
            });
        }
    }

    pub(super) fn malformed(&mut self, reason: &'static str) {
        self.note(MapProblem::Malformed {
            line: self.lines,
            reason,
        });
    }
}
