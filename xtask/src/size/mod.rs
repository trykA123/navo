use std::path::PathBuf;

mod check;
mod count;
mod exceptions;
mod run;

pub use check::check_files;
pub use count::count_code_lines;
pub use exceptions::parse_exceptions;
pub use run::run;

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

#[cfg(test)]
mod tests;
