mod floors;
mod report;
mod run;

pub use floors::{misses, parse_floors};
pub use report::crate_line_coverage;
pub use run::run;

#[derive(Debug, PartialEq)]
pub enum CoverageError {
    InvalidFloors(String),
    InvalidReport(String),
}

impl std::fmt::Display for CoverageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFloors(message) | Self::InvalidReport(message) => {
                formatter.write_str(message)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloorMiss {
    pub crate_name: String,
    pub actual: Option<f64>,
    pub floor: f64,
}

#[cfg(test)]
mod tests;
