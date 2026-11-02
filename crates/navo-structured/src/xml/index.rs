use std::sync::atomic::{AtomicBool, Ordering};

use quick_xml::Reader;
use quick_xml::events::Event;

use super::{Label, element, label_of, reference_text};
use crate::ParseError;
use crate::preview::{PREVIEW_CHARS, squeeze_into};
use crate::source::{RangeSource, ReadAt, Watch, line_column_at};
use crate::tree::{Flavor, Lazy, NodeId, Span, Tree};

const PROGRESS_EVERY: u64 = 8192;
pub const MAX_ENTRIES: usize = 100_000;
const MAX_EVENT_BYTES: u64 = 16 * 1024 * 1024;
const EVENT_TOO_LARGE: &str = "an XML node is over 16 MB, too large to show as a tree";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub span: Span,
    pub name: String,
    pub attrs: String,
    pub text: String,
    pub children: u32,
}

impl Entry {
    fn open(label: Label, start: u64) -> Self {
        Self {
            span: Span { start, end: start },
            name: label.name,
            attrs: label.attrs,
            text: label.text,
            children: 0,
        }
    }

    fn label(&self) -> Label {
        Label {
            name: self.name.clone(),
            attrs: self.attrs.clone(),
            text: self.text.clone(),
        }
    }

    fn heap_bytes(&self) -> usize {
        self.name.capacity() + self.attrs.capacity() + self.text.capacity()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Children {
    pub entries: Vec<Entry>,
    pub more: u64,
}

impl Children {
    fn push(&mut self, entry: Entry) {
        if self.entries.len() < MAX_ENTRIES {
            self.entries.push(entry);
        } else {
            self.more += 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlIndex {
    pub len: u64,
    pub elements: u64,
    pub root: Entry,
    pub top: Children,
}

impl XmlIndex {
    #[must_use]
    pub fn size_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.root.heap_bytes()
            + self.top.entries.capacity() * std::mem::size_of::<Entry>()
            + self
                .top
                .entries
                .iter()
                .map(Entry::heap_bytes)
                .sum::<usize>()
    }

    #[must_use]
    pub fn tree(&self) -> Tree {
        let mut tree = Tree::new(Flavor::Xml);
        let label = self.root.label();
        if let Some(root) = tree.push(None, element(&label)) {
            let _ = push_entries(&mut tree, root, &self.top);
        }
        tree
    }
}

fn push_entries(tree: &mut Tree, parent: NodeId, children: &Children) -> bool {
    for entry in &children.entries {
        let label = entry.label();
        let Some(id) = tree.push(Some(parent), element(&label)) else {
            return false;
        };
        tree.set_lazy(
            id,
            Lazy {
                span: entry.span,
                count: entry.children,
            },
        );
    }
    if children.more == 0 {
        return true;
    }
    let label = Label {
        name: format!("… {} more", children.more),
        ..Label::default()
    };
    tree.push(Some(parent), element(&label)).is_some()
}

#[must_use]
pub fn attach_children(tree: &mut Tree, parent: NodeId, children: &Children) -> bool {
    tree.take_lazy(parent).is_none() || push_entries(tree, parent, children)
}

struct Scanned {
    root: Entry,
    children: Children,
    elements: u64,
}

#[derive(Default)]
struct Scanner {
    depth: u32,
    root: Option<Entry>,
    child: Option<Entry>,
    children: Children,
    elements: u64,
}

enum Flow {
    More,
    Done,
}

impl Scanner {
    fn start(&mut self, label: Label, at: Span, is_empty: bool) -> Flow {
        self.elements += 1;
        match self.depth {
            0 => {
                let mut root = Entry::open(label, at.start);
                if is_empty {
                    root.span.end = at.end;
                    self.root = Some(root);
                    return Flow::Done;
                }
                self.root = Some(root);
            }
            1 => {
                if let Some(root) = self.root.as_mut() {
                    root.children = root.children.saturating_add(1);
                }
                let mut child = Entry::open(label, at.start);
                if is_empty {
                    child.span.end = at.end;
                    self.children.push(child);
                    return Flow::More;
                }
                self.child = Some(child);
            }
            2 => {
                if let Some(child) = self.child.as_mut() {
                    child.children = child.children.saturating_add(1);
                }
            }
            _ => {}
        }
        if !is_empty {
            self.depth += 1;
        }
        Flow::More
    }

    fn end(&mut self, end: u64) -> Flow {
        self.depth = self.depth.saturating_sub(1);
        match self.depth {
            0 => {
                if let Some(root) = self.root.as_mut() {
                    root.span.end = end;
                }
                Flow::Done
            }
            1 => {
                if let Some(mut child) = self.child.take() {
                    child.span.end = end;
                    self.children.push(child);
                }
                Flow::More
            }
            _ => Flow::More,
        }
    }

    fn text(&mut self, text: &str) {
        let target = match self.depth {
            1 => self.root.as_mut(),
            2 => self.child.as_mut(),
            _ => None,
        };
        if let Some(entry) = target {
            squeeze_into(&mut entry.text, text, PREVIEW_CHARS);
        }
    }

    fn event(&mut self, event: Event<'_>, at: Span) -> Option<Flow> {
        let flow = match event {
            Event::Start(start) => self.start(label_of(&start), at, false),
            Event::Empty(start) => self.start(label_of(&start), at, true),
            Event::End(_) => self.end(at.end),
            Event::Text(text) => {
                self.text(&String::from_utf8_lossy(&text));
                Flow::More
            }
            Event::CData(data) => {
                self.text(&String::from_utf8_lossy(&data));
                Flow::More
            }
            Event::GeneralRef(reference) => {
                self.text(&reference_text(&reference));
                Flow::More
            }
            Event::Eof => return None,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => Flow::More,
        };
        Some(flow)
    }
}

fn located(read: &mut ReadAt<'_>, message: String, offset: u64) -> ParseError {
    let (line, column) = line_column_at(read, offset);
    ParseError::new(message, line, column)
}

enum Ended {
    Done,
    Cancelled,
    Failed(String, u64),
}

fn run(
    mut reader: Reader<RangeSource<'_, '_>>,
    scanner: &mut Scanner,
    span: Span,
    watch: &mut Watch<'_>,
) -> Ended {
    let mut buf = Vec::new();
    let mut next_report = PROGRESS_EVERY;
    loop {
        if watch.cancel.load(Ordering::Relaxed) {
            return Ended::Cancelled;
        }
        let start = span.start + reader.buffer_position();
        reader
            .get_mut()
            .limit_from_here(MAX_EVENT_BYTES, EVENT_TOO_LARGE);
        let event = match reader.read_event_into(&mut buf) {
            Ok(event) => event,
            Err(error) => {
                return Ended::Failed(error.to_string(), span.start + reader.error_position());
            }
        };
        let end = span.start + reader.buffer_position();
        match scanner.event(event, Span { start, end }) {
            Some(Flow::More) => {}
            Some(Flow::Done) => return Ended::Done,
            None => {
                let (message, offset) = unclosed(scanner, end);
                return Ended::Failed(message, offset);
            }
        }
        buf.clear();
        if scanner.elements >= next_report {
            next_report = scanner.elements + PROGRESS_EVERY;
            (watch.progress)(scanner.elements);
        }
    }
}

fn scan(
    read: &mut ReadAt<'_>,
    span: Span,
    watch: &mut Watch<'_>,
) -> Result<Option<Scanned>, ParseError> {
    let mut scanner = Scanner::default();
    let reader = Reader::from_reader(RangeSource::new(read, span.start, span.end));
    match run(reader, &mut scanner, span, watch) {
        Ended::Done => {}
        Ended::Cancelled => return Ok(None),
        Ended::Failed(message, offset) => return Err(located(read, message, offset)),
    }
    let root = scanner
        .root
        .ok_or_else(|| ParseError::new("no root element", 1, 1))?;
    Ok(Some(Scanned {
        root,
        children: scanner.children,
        elements: scanner.elements,
    }))
}

fn unclosed(scanner: &Scanner, end: u64) -> (String, u64) {
    let message = match (&scanner.root, &scanner.child) {
        (None, _) => "no root element".to_owned(),
        (Some(_), Some(child)) => format!("<{}> is never closed", child.name),
        (Some(root), None) => format!("<{}> is never closed", root.name),
    };
    (message, end)
}

pub fn build_index(
    read: &mut ReadAt<'_>,
    len: u64,
    watch: &mut Watch<'_>,
) -> Result<Option<XmlIndex>, ParseError> {
    let span = Span { start: 0, end: len };
    let Some(scanned) = scan(read, span, watch)? else {
        return Ok(None);
    };
    (watch.progress)(scanned.elements);
    Ok(Some(XmlIndex {
        len,
        elements: scanned.elements,
        root: scanned.root,
        top: scanned.children,
    }))
}

pub fn scan_children(
    read: &mut ReadAt<'_>,
    span: Span,
    cancel: &AtomicBool,
) -> Result<Option<Children>, ParseError> {
    let mut ignore = |_: u64| {};
    let mut watch = Watch {
        cancel,
        progress: &mut ignore,
    };
    Ok(scan(read, span, &mut watch)?.map(|scanned| scanned.children))
}
