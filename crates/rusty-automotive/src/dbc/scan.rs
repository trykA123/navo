use std::io::BufRead;

use super::DbcModel;
use crate::Watch;
use crate::description::{DescriptionError, Lexer, MAX_RECORD, Token, TokenKind};

#[derive(Default)]
struct Record {
    text: String,
    line: u64,
    kind: String,
    overflow: bool,
}

impl Record {
    fn statement(&self) -> bool {
        !matches!(self.kind.as_str(), "BO_" | "SG_" | "BU_:" | "VERSION")
    }

    fn push(&mut self, token: &Token) {
        if self.text.len() + token.text.len() + 1 > MAX_RECORD {
            self.overflow = true;
        }
        if !self.overflow {
            self.text.push_str(&token.text);
            self.text.push(' ');
        }
    }
}

#[derive(Default)]
struct Scanner {
    model: DbcModel,
    record: Record,
    current: Option<usize>,
    symbols: bool,
    first: bool,
}

impl Scanner {
    fn token(&mut self, token: Token) {
        if token.kind == TokenKind::Problem {
            self.model.report.problem(token.line, token.text);
            self.record.overflow = true;
            return;
        }
        if token.kind == TokenKind::EndLine {
            if !self.record.kind.is_empty() && !self.record.statement() {
                self.flush();
            } else if !self.record.kind.is_empty() {
                self.record.push(&token);
            }
            self.first = true;
            return;
        }
        if self.start(&token) {
            self.record.push(&token);
        }
        if token.kind == TokenKind::Semicolon {
            self.flush();
        }
        self.first = false;
    }

    fn start(&mut self, token: &Token) -> bool {
        let kind = token.text.as_str();
        if self.first && matches!(kind, "NS_" | "NS_:") {
            self.symbols = true;
            return false;
        }
        if self.symbols {
            if !matches!(kind, "BS_:" | "BU_:" | "BO_" | "VERSION") {
                return false;
            }
            self.symbols = false;
        }
        let known = matches!(
            kind,
            "VERSION"
                | "BU_:"
                | "BO_"
                | "SG_"
                | "VAL_"
                | "VAL_TABLE_"
                | "CM_"
                | "BA_"
                | "BA_DEF_"
                | "BA_DEF_DEF_"
                | "SG_MUL_VAL_"
        );
        if known && (self.first || self.record.kind.is_empty()) {
            if !self.record.kind.is_empty() {
                self.flush();
            }
            self.record = Record {
                kind: kind.into(),
                line: token.line,
                ..Record::default()
            };
        }
        !self.record.kind.is_empty()
    }

    fn flush(&mut self) {
        let record = std::mem::take(&mut self.record);
        if record.kind.is_empty() {
            return;
        }
        if record.kind == "BO_" {
            self.current = None;
        }
        if record.overflow {
            self.model.report.problem(
                record.line,
                "record exceeds limits or contains an invalid token",
            );
            return;
        }
        if !valid_ids(&record.text) {
            self.model
                .report
                .problem(record.line, "invalid CAN message ID");
            return;
        }
        let is_signal = record.kind == "SG_";
        let text = if is_signal {
            format!("BO_ 0 Frame: 8 Vector__XXX\n{}\n", record.text.trim_end())
        } else {
            format!("{}\n", record.text.trim_end())
        };
        match can_dbc::Dbc::try_from(text.as_str()) {
            Ok(parsed)
                if self.model.report.retain_rows(
                    record.line,
                    record.text.len(),
                    super::convert::rows(&parsed),
                ) =>
            {
                self.model
                    .take(parsed, &mut self.current, (record.line, is_signal))
            }
            Ok(_) => {}
            Err(error) => {
                let reason = error.to_string();
                let relative = error_line(&reason).saturating_sub(u64::from(is_signal));
                self.model.report.problem(
                    record.line + relative.saturating_sub(1),
                    reason.chars().take(1024).collect::<String>(),
                );
            }
        }
    }
}

fn valid_ids(text: &str) -> bool {
    let words: Vec<_> = text.split_whitespace().take(5).collect();
    let at = match words.as_slice() {
        ["BO_" | "SG_MUL_VAL_", ..] => Some(1),
        ["VAL_", second, ..] if second.bytes().all(|byte| byte.is_ascii_digit()) => Some(1),
        ["CM_", "BO_" | "SG_", ..] | ["BA_", _, "BO_" | "SG_", ..] => words
            .iter()
            .position(|word| matches!(*word, "BO_" | "SG_"))
            .map(|at| at + 1),
        _ => None,
    };
    at.is_none_or(|at| {
        words
            .get(at)
            .and_then(|word| word.parse::<u32>().ok())
            .is_some_and(|id| {
                id <= 2047 || ((id & (1 << 31)) != 0 && (id & !(1 << 31)) <= 536_870_911)
            })
    })
}

fn error_line(reason: &str) -> u64 {
    reason
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("--> ")?
                .split(':')
                .next()?
                .parse()
                .ok()
        })
        .unwrap_or(1)
}

pub fn index_dbc(read: impl BufRead, watch: Watch<'_>) -> Result<DbcModel, DescriptionError> {
    let mut lexer = Lexer::new(read, watch);
    let mut scanner = Scanner {
        first: true,
        ..Scanner::default()
    };
    while let Some(token) = lexer.next()? {
        scanner.token(token);
    }
    scanner.flush();
    scanner.model.finish();
    (scanner.model.report.bytes, scanner.model.report.lines) = lexer.position();
    lexer.finish()?;
    Ok(scanner.model)
}
