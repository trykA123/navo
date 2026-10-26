mod lexer;
mod rows;

pub(crate) use lexer::{Lexer, Token, TokenKind};
pub use rows::DescRow;

pub const MAX_PROBLEMS: usize = 1000;
pub(crate) const MAX_RECORD: usize = 256 * 1024;
const MAX_ROWS: usize = 250_000;
const MAX_TEXT: usize = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum DescriptionError {
    #[error("reading the description file: {0}")]
    Read(#[from] std::io::Error),
    #[error("indexing was cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptionProblem {
    pub line: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct DescriptionReport {
    pub problems: Vec<DescriptionProblem>,
    pub problem_count: u64,
    pub omitted: u64,
    pub bytes: u64,
    pub lines: u64,
    rows: usize,
    text: usize,
}

impl DescriptionReport {
    pub(crate) fn problem(&mut self, line: u64, reason: impl Into<String>) {
        self.problem_count += 1;
        if self.problems.len() < MAX_PROBLEMS {
            let mut reason = reason.into();
            reason.truncate(reason.floor_char_boundary(1024));
            self.problems.push(DescriptionProblem { line, reason });
        }
    }

    pub(crate) fn retain(&mut self, line: u64, bytes: usize) -> bool {
        self.retain_rows(line, bytes, 1)
    }

    pub(crate) fn retain_rows(&mut self, line: u64, bytes: usize, rows: usize) -> bool {
        if self.rows.saturating_add(rows) > MAX_ROWS || self.text.saturating_add(bytes) > MAX_TEXT {
            if self.omitted == 0 {
                self.problem(
                    line,
                    "description retention limit reached (250000 entries or 32 MiB of text)",
                );
            }
            self.omitted += 1;
            return false;
        }
        self.rows += rows;
        self.text += bytes;
        true
    }
}
