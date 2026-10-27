mod convert;
mod outline;
mod rows;
mod scan;

use crate::DescriptionReport;
use std::sync::Arc;

pub use outline::{Links, links, outline};
pub use rows::rows;
pub use scan::index_a2l;
pub const A2L_EXTENSIONS: [&str; 1] = ["a2l"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InterfaceData {
    pub present: bool,
    pub xcp: bool,
    pub ccp: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub line: u64,
    pub interface_data: InterfaceData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub module: usize,
    pub line: u64,
    pub name: String,
    pub address: Option<u32>,
    pub data_type: String,
    pub conversion: String,
    pub unit: Option<Arc<str>>,
    pub lower_limit: f64,
    pub upper_limit: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Characteristic {
    pub module: usize,
    pub line: u64,
    pub name: String,
    pub kind: String,
    pub axes: Vec<AxisDescription>,
    pub address: u32,
    pub record_layout: String,
    pub conversion: String,
    pub unit: Option<Arc<str>>,
    pub lower_limit: f64,
    pub upper_limit: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisDescription {
    pub input_quantity: String,
    pub conversion: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompuMethod {
    pub module: usize,
    pub line: u64,
    pub name: String,
    pub conversion_type: String,
    pub coeffs: Option<[f64; 6]>,
    pub coeffs_linear: Option<[f64; 2]>,
    pub unit: Arc<str>,
}

#[derive(Debug, Clone, Default)]
pub struct A2lModel {
    pub project: Option<String>,
    pub version: Option<(u16, u16)>,
    pub modules: Vec<Module>,
    pub measurements: Vec<Measurement>,
    pub characteristics: Vec<Characteristic>,
    pub compu_methods: Vec<CompuMethod>,
    pub report: DescriptionReport,
}

#[must_use]
pub fn sniff_a2l(head: &[u8]) -> bool {
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
    while let Ok(Some(token)) = lexer.significant() {
        if token.kind != TokenKind::Word {
            continue;
        }
        if token.text == "ASAP2_VERSION" {
            return version_number(lexer.significant()) && version_number(lexer.significant());
        }
        if token.text == "/begin"
            && lexer.significant().is_ok_and(|token| {
                token.is_some_and(|token| token.kind == TokenKind::Word && token.text == "PROJECT")
            })
        {
            return true;
        }
    }
    false
}

fn version_number(
    token: Result<Option<crate::description::Token>, crate::DescriptionError>,
) -> bool {
    matches!(token, Ok(Some(token)) if token.kind == crate::description::TokenKind::Word && token.text.parse::<u16>().is_ok())
}
