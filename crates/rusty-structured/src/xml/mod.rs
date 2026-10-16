mod index;

use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

pub use index::{
    Children, Entry, MAX_ENTRIES, XmlIndex, attach_children, build_index, scan_children,
};

use crate::ParseError;
use crate::preview::{PREVIEW_CHARS, squeeze_into};
use crate::tree::{Flavor, Key, NewNode, NodeId, NodeKind, Tree};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Label {
    pub name: String,
    pub attrs: String,
    pub text: String,
}

pub(crate) fn label_of(start: &BytesStart<'_>) -> Label {
    let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
    let mut attrs = String::new();
    for attribute in start.attributes().flatten() {
        let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
        let value = attribute
            .normalized_value(XmlVersion::default())
            .map_or_else(
                |_| String::from_utf8_lossy(&attribute.value).into_owned(),
                std::borrow::Cow::into_owned,
            );
        if !attrs.is_empty() {
            squeeze_into(&mut attrs, " ", PREVIEW_CHARS);
        }
        squeeze_into(&mut attrs, &format!("{key}=\"{value}\""), PREVIEW_CHARS);
    }
    Label {
        name,
        attrs,
        text: String::new(),
    }
}

pub(crate) fn reference_text(reference: &BytesRef<'_>) -> String {
    if let Ok(Some(ch)) = reference.resolve_char_ref() {
        return ch.to_string();
    }
    let name = String::from_utf8_lossy(reference);
    match name.as_ref() {
        "amp" => "&".to_owned(),
        "lt" => "<".to_owned(),
        "gt" => ">".to_owned(),
        "quot" => "\"".to_owned(),
        "apos" => "'".to_owned(),
        other => format!("&{other};"),
    }
}

pub(crate) fn add_text(label: &mut Label, text: &str) {
    squeeze_into(&mut label.text, text, PREVIEW_CHARS);
}

fn element<'a>(label: &'a Label) -> NewNode<'a> {
    NewNode {
        kind: NodeKind::Element,
        key: Key::Name(&label.name),
        value: label.text.trim(),
        attrs: &label.attrs,
    }
}

struct Full<'b> {
    bytes: &'b [u8],
    tree: Tree,
    stack: Vec<(NodeId, Label)>,
}

impl Full<'_> {
    fn error(&self, message: impl Into<String>, offset: u64) -> ParseError {
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        ParseError::at_offset(message, self.bytes, offset)
    }

    fn open(&mut self, label: Label, offset: u64) -> Result<NodeId, ParseError> {
        let parent = self.stack.last().map(|(id, _)| *id);
        if parent.is_none() && self.tree.root().is_some() {
            return Err(self.error("more than one root element", offset));
        }
        self.tree
            .push(parent, element(&label))
            .ok_or_else(|| self.error("too many elements to show as a tree", offset))
    }

    fn close(&mut self) {
        if let Some((id, label)) = self.stack.pop() {
            self.tree.set_value(id, label.text.trim());
        }
    }

    fn text(&mut self, text: &str) {
        if let Some((_, label)) = self.stack.last_mut() {
            add_text(label, text);
        }
    }

    fn event(&mut self, event: Event<'_>, offset: u64) -> Result<bool, ParseError> {
        match event {
            Event::Start(start) => {
                let label = label_of(&start);
                let id = self.open(label.clone(), offset)?;
                self.stack.push((id, label));
            }
            Event::Empty(start) => {
                self.open(label_of(&start), offset)?;
            }
            Event::End(_) => self.close(),
            Event::Text(text) => self.text(&String::from_utf8_lossy(&text)),
            Event::CData(data) => self.text(&String::from_utf8_lossy(&data)),
            Event::GeneralRef(reference) => self.text(&reference_text(&reference)),
            Event::Eof => return Ok(false),
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
        Ok(true)
    }
}

pub fn parse_xml(bytes: &[u8]) -> Result<Tree, ParseError> {
    let mut reader = Reader::from_reader(bytes);
    let mut full = Full {
        bytes,
        tree: Tree::new(Flavor::Xml),
        stack: Vec::new(),
    };
    let mut buf = Vec::new();
    loop {
        let offset = reader.buffer_position();
        let event = match reader.read_event_into(&mut buf) {
            Ok(event) => event,
            Err(error) => return Err(full.error(error.to_string(), reader.error_position())),
        };
        if !full.event(event, offset)? {
            break;
        }
        buf.clear();
    }
    let end = reader.buffer_position();
    if let Some((_, label)) = full.stack.last() {
        return Err(full.error(format!("<{}> is never closed", label.name), end));
    }
    if full.tree.root().is_none() {
        return Err(full.error("no root element", end));
    }
    Ok(full.tree)
}

#[cfg(test)]
mod tests;
