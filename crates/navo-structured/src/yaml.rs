use saphyr_parser::{Event, Parser, ScalarStyle, ScanError};

use crate::ParseError;
use crate::tree::{Flavor, Key, NewNode, NodeId, NodeKind, Tree};

const COMPLEX_KEY: &str = "?";

enum Frame {
    Seq { id: NodeId, index: u32 },
    Map { id: NodeId, key: Option<String> },
    Skip { depth: u32 },
}

struct Builder {
    tree: Tree,
    stack: Vec<Frame>,
}

fn scan_error(error: &ScanError) -> ParseError {
    let marker = error.marker();
    ParseError::new(error.info(), marker.line() as u64, marker.col() as u64 + 1)
}

fn is_number(text: &str) -> bool {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    let starts_right = digits
        .strip_prefix('.')
        .unwrap_or(digits)
        .starts_with(|ch: char| ch.is_ascii_digit());
    starts_right && (text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok())
}

fn scalar_kind(value: &str, style: ScalarStyle) -> NodeKind {
    if style != ScalarStyle::Plain {
        return NodeKind::String;
    }
    match value {
        "" | "~" | "null" | "Null" | "NULL" => NodeKind::Null,
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => NodeKind::Bool,
        _ if is_number(value) => NodeKind::Number,
        _ => NodeKind::String,
    }
}

impl Builder {
    fn too_many() -> ParseError {
        ParseError::new("too many values to show as a tree", 0, 0)
    }

    fn take_key(&mut self) -> (Option<NodeId>, Option<Child>) {
        match self.stack.last_mut() {
            Some(Frame::Seq { id, index }) => {
                let child = Child::Index(*index);
                *index = index.saturating_add(1);
                (Some(*id), Some(child))
            }
            Some(Frame::Map { id, key }) => (Some(*id), key.take().map(Child::Name)),
            Some(Frame::Skip { .. }) | None => (None, None),
        }
    }

    fn wants_key(&self) -> bool {
        matches!(self.stack.last(), Some(Frame::Map { key: None, .. }))
    }

    fn set_key(&mut self, name: String) {
        if let Some(Frame::Map { key, .. }) = self.stack.last_mut() {
            *key = Some(name);
        }
    }

    fn push(&mut self, kind: NodeKind, value: &str) -> Result<Option<NodeId>, ParseError> {
        let (parent, child) = self.take_key();
        let Some(child) = child else {
            return Ok(None);
        };
        let key = match &child {
            Child::Index(index) => Key::Index(*index),
            Child::Name(name) => Key::Name(name),
        };
        let node = NewNode {
            kind,
            key,
            value,
            attrs: "",
        };
        self.tree
            .push(parent, node)
            .map(Some)
            .ok_or_else(Self::too_many)
    }

    fn open(&mut self, is_map: bool) -> Result<(), ParseError> {
        if let Some(Frame::Skip { depth }) = self.stack.last_mut() {
            *depth += 1;
            return Ok(());
        }
        if self.wants_key() {
            self.set_key(COMPLEX_KEY.to_owned());
            self.stack.push(Frame::Skip { depth: 1 });
            return Ok(());
        }
        let kind = if is_map {
            NodeKind::Object
        } else {
            NodeKind::Array
        };
        let Some(id) = self.push(kind, "")? else {
            return Ok(());
        };
        self.stack.push(if is_map {
            Frame::Map { id, key: None }
        } else {
            Frame::Seq { id, index: 0 }
        });
        Ok(())
    }

    fn close(&mut self) {
        if let Some(Frame::Skip { depth }) = self.stack.last_mut()
            && *depth > 1
        {
            *depth -= 1;
            return;
        }
        self.stack.pop();
    }

    fn scalar(&mut self, value: &str, kind: NodeKind) -> Result<(), ParseError> {
        if matches!(self.stack.last(), Some(Frame::Skip { .. })) {
            return Ok(());
        }
        if self.wants_key() {
            self.set_key(value.to_owned());
            return Ok(());
        }
        self.push(kind, value).map(|_| ())
    }

    fn event(&mut self, event: Event<'_>) -> Result<(), ParseError> {
        match event {
            Event::Scalar(value, style, _, _) => self.scalar(&value, scalar_kind(&value, style)),
            Event::Alias(_) => self.scalar("*alias", NodeKind::String),
            Event::SequenceStart(..) => self.open(false),
            Event::MappingStart(..) => self.open(true),
            Event::SequenceEnd | Event::MappingEnd => {
                self.close();
                Ok(())
            }
            Event::Nothing
            | Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart(_)
            | Event::DocumentEnd => Ok(()),
        }
    }
}

enum Child {
    Index(u32),
    Name(String),
}

pub fn parse_yaml(bytes: &[u8]) -> Result<Tree, ParseError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| ParseError::at_offset("not valid UTF-8", bytes, error.valid_up_to()))?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut tree = Tree::new(Flavor::Json);
    let documents = tree
        .push(
            None,
            NewNode {
                kind: NodeKind::Array,
                key: Key::Root,
                value: "",
                attrs: "",
            },
        )
        .ok_or_else(Builder::too_many)?;
    let mut builder = Builder {
        tree,
        stack: vec![Frame::Seq {
            id: documents,
            index: 0,
        }],
    };
    let mut parser = Parser::new_from_str(text);
    while let Some(next) = parser.next_event() {
        let (event, _) = next.map_err(|error| scan_error(&error))?;
        builder.event(event)?;
    }
    let mut tree = builder.tree;
    let first = tree.children(documents).next();
    if let (1, Some(only)) = (tree.child_count(documents), first) {
        tree.set_root(only);
    }
    Ok(tree)
}
