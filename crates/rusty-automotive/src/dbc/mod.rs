mod convert;
mod scan;

use crate::{CanId, DescriptionReport, Endian};
use std::collections::HashMap;
use std::ops::RangeInclusive;

pub use scan::index_dbc;
pub const DBC_EXTENSIONS: [&str; 1] = ["dbc"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    Unsigned(u64),
    Signed(i64),
    Float(f64),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SignalFormat {
    #[default]
    Integer,
    Float,
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Multiplexing {
    Plain,
    Switch,
    Value(u64),
    SwitchValue(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub name: String,
    pub start_bit: u64,
    pub length: u64,
    pub byte_order: Endian,
    pub signed: bool,
    pub format: SignalFormat,
    pub factor: f64,
    pub offset: f64,
    pub min: Number,
    pub max: Number,
    pub unit: String,
    pub receivers: Vec<String>,
    pub multiplexing: Multiplexing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub line: u64,
    pub id: CanId,
    pub name: String,
    pub dlc: u64,
    pub sender: Option<String>,
    pub cycle_time_ms: Option<f64>,
    pub signals: Vec<Signal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub value: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueTarget {
    Table(String),
    Signal { message: CanId, signal: String },
    Environment(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueTable {
    pub target: ValueTarget,
    pub values: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentTarget {
    File,
    Node(String),
    Message(CanId),
    Signal { message: CanId, signal: String },
    Environment(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub target: CommentTarget,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiplexRange {
    pub message: CanId,
    pub signal: String,
    pub switch: String,
    pub ranges: Vec<RangeInclusive<u64>>,
}

#[derive(Debug, Clone, Default)]
pub struct DbcModel {
    pub nodes: Vec<String>,
    pub messages: Vec<Message>,
    pub value_tables: Vec<ValueTable>,
    pub comments: Vec<Comment>,
    pub multiplex_ranges: Vec<MultiplexRange>,
    pub cycle_time_default_ms: Option<f64>,
    pub report: DescriptionReport,
    lookup: HashMap<CanId, usize>,
    cycles: HashMap<CanId, Option<f64>>,
    formats: HashMap<(CanId, String), SignalFormat>,
}

impl DbcModel {
    #[must_use]
    pub fn message(&self, id: CanId) -> Option<&Message> {
        self.lookup.get(&id).and_then(|at| self.messages.get(*at))
    }

    #[must_use]
    pub fn signals(&self, id: CanId) -> Option<&[Signal]> {
        self.message(id).map(|message| message.signals.as_slice())
    }

    fn finish(&mut self) {
        for message in &mut self.messages {
            message.cycle_time_ms = self
                .cycles
                .get(&message.id)
                .copied()
                .unwrap_or(self.cycle_time_default_ms);
        }
        self.apply_formats();
        self.cycles = HashMap::new();
        self.formats = HashMap::new();
    }

    fn apply_formats(&mut self) {
        if self.formats.is_empty() {
            return;
        }
        for message in &mut self.messages {
            for signal in &mut message.signals {
                let key = (message.id, std::mem::take(&mut signal.name));
                signal.format = self.formats.get(&key).copied().unwrap_or_default();
                signal.name = key.1;
            }
        }
    }
}

#[must_use]
pub fn sniff_dbc(head: &[u8]) -> bool {
    use crate::description::{Lexer, TokenKind};
    use std::sync::atomic::AtomicBool;
    let head = &head[..head.len().min(16 * 1024)];
    let cancel = AtomicBool::new(false);
    let mut progress = |_| {};
    let mut lexer = Lexer::new(
        head,
        crate::Watch {
            cancel: &cancel,
            progress: &mut progress,
        },
    );
    let mut line = String::new();
    while let Ok(Some(token)) = lexer.next() {
        if token.kind == TokenKind::EndLine {
            if recognized(&line) {
                return true;
            }
            line.clear();
        } else if token.kind == TokenKind::Problem {
            line.clear();
        } else {
            line.push_str(&token.text);
            line.push(' ');
        }
    }
    recognized(&line)
}

fn recognized(line: &str) -> bool {
    matches!(
        line.split_whitespace().next(),
        Some("VERSION" | "BO_" | "BU_:" | "BU_")
    ) && can_dbc::Dbc::try_from(format!("{}\n", line.trim_end()).as_str()).is_ok()
}
