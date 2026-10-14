use std::path::PathBuf;

pub const DEFAULT_LIMIT: usize = 400;
pub const CEILING: usize = 800;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    pub path: PathBuf,
    pub limit: usize,
    pub reason: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SizeError {
    InvalidToml(String),
    LimitAboveCeiling { path: PathBuf, limit: usize },
    MissingReason { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeViolation {
    pub path: PathBuf,
    pub lines: usize,
    pub limit: usize,
}

pub fn count_code_lines(_source: &str) -> usize {
    todo!()
}

pub fn parse_exceptions(_toml: &str) -> Result<Vec<Exception>, SizeError> {
    todo!()
}

pub fn check_files(_files: &[(PathBuf, String)], _exceptions: &[Exception]) -> Vec<SizeViolation> {
    todo!()
}

#[cfg(test)]
mod tests;
