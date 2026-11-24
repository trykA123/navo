//! XML syntax lint: turns quick-xml's first error or an unclosed root into a Diagnostic.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::{Diagnostic, Severity, char_range, position, split_bom};

/// Returns the first XML problem in `text`, or None when it parses.
pub(super) fn lint_xml(text: &str) -> Option<Diagnostic> {
    let (source, bom) = split_bom(text);
    let mut reader = Reader::from_str(source);
    let mut stack: Vec<String> = Vec::new();
    let mut seen = false;
    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(error) => {
                let offset = reader.error_position() as usize + bom;
                return Some(located(text, offset, &error.to_string()));
            }
        };
        match event {
            Event::Start(start) => {
                stack.push(name_of(&start));
                seen = true;
            }
            Event::Empty(_) => seen = true,
            Event::End(_) => {
                stack.pop();
            }
            Event::Eof => return eof(text, stack.last(), seen),
            _ => {}
        }
    }
}

/// Builds a diagnostic at byte `offset` from a parser error message.
fn located(text: &str, offset: usize, message: &str) -> Diagnostic {
    let (line, col) = position(text, offset);
    Diagnostic {
        range: char_range(text, offset),
        line,
        col,
        severity: Severity::Error,
        message: message.trim().to_owned(),
    }
}

/// Reports an unclosed innermost element at EOF, or a missing root when none was seen.
fn eof(text: &str, open: Option<&String>, seen: bool) -> Option<Diagnostic> {
    if let Some(name) = open {
        let (line, col) = position(text, text.len());
        return Some(Diagnostic {
            range: text.len()..text.len(),
            line,
            col,
            severity: Severity::Error,
            message: format!("unclosed element <{name}>"),
        });
    }
    (!seen).then(|| Diagnostic {
        range: 0..0,
        line: 1,
        col: 1,
        severity: Severity::Error,
        message: "no root element".to_owned(),
    })
}

/// UTF-8 name of a start tag without namespace resolution.
fn name_of(start: &BytesStart<'_>) -> String {
    start.name().as_ref().to_owned()
}
