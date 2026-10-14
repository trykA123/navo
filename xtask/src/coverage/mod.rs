use std::collections::BTreeMap;

#[derive(Debug, PartialEq)]
pub enum CoverageError {
    InvalidFloors(String),
    InvalidReport(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloorMiss {
    pub crate_name: String,
    pub actual: Option<f64>,
    pub floor: f64,
}

pub fn parse_floors(toml: &str) -> Result<BTreeMap<String, f64>, CoverageError> {
    let _ = toml;
    todo!()
}

pub fn crate_line_coverage(llvm_cov_json: &str) -> Result<BTreeMap<String, f64>, CoverageError> {
    let _ = llvm_cov_json;
    todo!()
}

pub fn misses(coverage: &BTreeMap<String, f64>, floors: &BTreeMap<String, f64>) -> Vec<FloorMiss> {
    let _ = (coverage, floors);
    todo!()
}

pub fn run(args: &[String]) -> std::process::ExitCode {
    let _ = args;
    todo!()
}

#[cfg(test)]
mod tests;
