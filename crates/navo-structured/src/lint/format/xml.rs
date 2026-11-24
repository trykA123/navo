//! XML pretty-printer: keeps mixed content and comments intact, indents element nesting.

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::lint::split_bom;

/// One parsed event and the source byte span it covers.
struct Item<'a> {
    event: Event<'a>,
    start: usize,
    end: usize,
}

/// Matching End index per Start plus a mixed-content mark per Start.
struct Shape {
    ends: Vec<Option<usize>>,
    mixed: Vec<bool>,
}

/// Returns an indented copy, or None when reading fails so `format` keeps the input.
pub(super) fn format_xml(text: &str) -> Option<String> {
    let (source, bom) = split_bom(text);
    let items = read_events(source)?;
    let shape = analyse(&items);
    let mut out = String::with_capacity(source.len());
    let mut depth = 0;
    let mut at = 0;
    while at < items.len() {
        if shape.mixed[at] {
            let end = shape.ends[at]?;
            line_start(&mut out, depth);
            out.push_str(source.get(items[at].start..items[end].end)?);
            at = end + 1;
            continue;
        }
        let item = items.get(at)?;
        match &item.event {
            Event::Start(_) => {
                line_start(&mut out, depth);
                out.push_str(source.get(item.start..item.end)?);
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                line_start(&mut out, depth);
                out.push_str(source.get(item.start..item.end)?);
            }
            Event::Empty(_) => {
                line_start(&mut out, depth);
                out.push_str(source.get(item.start..item.end)?);
            }
            Event::Text(content) => {
                if !content.trim().is_empty() {
                    out.push_str(source.get(item.start..item.end)?);
                }
            }
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {
                line_start(&mut out, depth);
                out.push_str(source.get(item.start..item.end)?);
            }
            Event::CData(_) | Event::GeneralRef(_) => {
                out.push_str(source.get(item.start..item.end)?);
            }
            Event::Eof => break,
        }
        at += 1;
    }
    Some(super::finish(&out, text, bom))
}

/// Reads all events up to EOF, recording each event's byte span.
fn read_events(source: &str) -> Option<Vec<Item<'_>>> {
    let mut reader = Reader::from_str(source);
    let mut items = Vec::new();
    loop {
        let start = reader.buffer_position() as usize;
        let event = reader.read_event().ok()?;
        let end = reader.buffer_position() as usize;
        if matches!(event, Event::Eof) {
            break;
        }
        items.push(Item { event, start, end });
    }
    Some(items)
}

/// Pairs Start with End and marks elements that hold non-whitespace text or CDATA.
fn analyse(items: &[Item<'_>]) -> Shape {
    let mut ends = vec![None; items.len()];
    let mut mixed = vec![false; items.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (at, item) in items.iter().enumerate() {
        match &item.event {
            Event::Start(_) => stack.push(at),
            Event::End(_) => {
                if let Some(open) = stack.pop() {
                    ends[open] = Some(at);
                }
            }
            Event::Text(content) if !content.trim().is_empty() => {
                if let Some(open) = stack.last() {
                    mixed[*open] = true;
                }
            }
            Event::CData(_) | Event::GeneralRef(_) => {
                if let Some(open) = stack.last() {
                    mixed[*open] = true;
                }
            }
            _ => {}
        }
    }
    Shape { ends, mixed }
}

/// Starts a line indented by `depth` two-space units unless this is the first output.
fn line_start(out: &mut String, depth: usize) {
    if !out.is_empty() {
        out.push('\n');
    }
    for _ in 0..depth {
        out.push_str("  ");
    }
}
