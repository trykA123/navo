use std::fmt;

use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};

use crate::ParseError;
use crate::tree::{Flavor, Key, NewNode, NodeId, NodeKind, Tree};

const BOM: &[u8] = b"\xEF\xBB\xBF";

enum OwnedKey {
    Root,
    Name(String),
    Index(u32),
}

struct Seed<'t> {
    tree: &'t mut Tree,
    parent: Option<NodeId>,
    key: OwnedKey,
}

impl Seed<'_> {
    fn push<E: serde::de::Error>(&mut self, kind: NodeKind, value: &str) -> Result<NodeId, E> {
        let key = match &self.key {
            OwnedKey::Root => Key::Root,
            OwnedKey::Name(name) => Key::Name(name),
            OwnedKey::Index(index) => Key::Index(*index),
        };
        let node = NewNode {
            kind,
            key,
            value,
            attrs: "",
        };
        self.tree
            .push(self.parent, node)
            .ok_or_else(|| E::custom("too many values to show as a tree"))
    }

    fn leaf<E: serde::de::Error>(mut self, kind: NodeKind, value: &str) -> Result<(), E> {
        self.push(kind, value).map(|_| ())
    }
}

impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<(), E> {
        self.leaf(NodeKind::Bool, if value { "true" } else { "false" })
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<(), E> {
        self.leaf(NodeKind::Number, &value.to_string())
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<(), E> {
        self.leaf(NodeKind::Number, &value.to_string())
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<(), E> {
        self.leaf(NodeKind::Number, &value.to_string())
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<(), E> {
        self.leaf(NodeKind::String, value)
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        self.leaf(NodeKind::Null, "null")
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<(), E> {
        self.leaf(NodeKind::Null, "null")
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<(), A::Error> {
        let id = self.push(NodeKind::Array, "")?;
        let mut index = 0u32;
        loop {
            let seed = Seed {
                tree: &mut *self.tree,
                parent: Some(id),
                key: OwnedKey::Index(index),
            };
            if seq.next_element_seed(seed)?.is_none() {
                return Ok(());
            }
            index = index.saturating_add(1);
        }
    }

    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<(), A::Error> {
        let id = self.push(NodeKind::Object, "")?;
        while let Some(name) = map.next_key::<String>()? {
            let seed = Seed {
                tree: &mut *self.tree,
                parent: Some(id),
                key: OwnedKey::Name(name),
            };
            map.next_value_seed(seed)?;
        }
        Ok(())
    }
}

fn error_of(error: &serde_json::Error) -> ParseError {
    let text = error.to_string();
    let suffix = format!(" at line {} column {}", error.line(), error.column());
    let message = text.strip_suffix(&suffix).unwrap_or(&text);
    ParseError::new(message, error.line() as u64, error.column() as u64)
}

pub fn parse_json(bytes: &[u8]) -> Result<Tree, ParseError> {
    let bytes = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let mut tree = Tree::new(Flavor::Json);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let seed = Seed {
        tree: &mut tree,
        parent: None,
        key: OwnedKey::Root,
    };
    seed.deserialize(&mut deserializer)
        .map_err(|error| error_of(&error))?;
    deserializer.end().map_err(|error| error_of(&error))?;
    Ok(tree)
}

pub fn parse_jsonc(bytes: &[u8]) -> Result<Tree, ParseError> {
    let bytes = bytes.strip_prefix(BOM).unwrap_or(bytes);
    parse_json(&strip_trailing_commas(&strip_comments(bytes)))
}

fn strip_comments(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let mut at = 0;
    let mut in_string = false;
    while at < out.len() {
        let byte = out.get(at).copied().unwrap_or_default();
        let next = out.get(at + 1).copied();
        if in_string {
            match byte {
                b'\\' => at += 1,
                b'"' => in_string = false,
                _ => {}
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == b'/' && next == Some(b'/') {
            let end = out
                .get(at..)
                .and_then(|rest| rest.iter().position(|byte| *byte == b'\n'))
                .map_or(out.len(), |offset| at + offset);
            blank(&mut out, at, end);
            at = end;
            continue;
        } else if byte == b'/' && next == Some(b'*') {
            let end = out
                .get(at + 2..)
                .and_then(|rest| rest.windows(2).position(|pair| pair == b"*/"))
                .map_or(out.len(), |offset| at + 2 + offset + 2);
            blank(&mut out, at, end);
            at = end;
            continue;
        }
        at += 1;
    }
    out
}

fn blank(bytes: &mut [u8], start: usize, end: usize) {
    if let Some(range) = bytes.get_mut(start..end) {
        range
            .iter_mut()
            .filter(|byte| **byte != b'\n')
            .for_each(|byte| *byte = b' ');
    }
}

fn strip_trailing_commas(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let mut in_string = false;
    let mut escaped = false;
    for at in 0..out.len() {
        let byte = out.get(at).copied().unwrap_or_default();
        if in_string {
            in_string = escaped || byte != b'"';
            escaped = !escaped && byte == b'\\';
            continue;
        }
        if byte == b'"' {
            in_string = true;
        } else if byte == b','
            && closes_next(&out, at + 1)
            && let Some(slot) = out.get_mut(at)
        {
            *slot = b' ';
        }
    }
    out
}

fn closes_next(bytes: &[u8], from: usize) -> bool {
    bytes
        .get(from..)
        .and_then(|rest| rest.iter().find(|byte| !byte.is_ascii_whitespace()))
        .is_some_and(|byte| *byte == b'}' || *byte == b']')
}
