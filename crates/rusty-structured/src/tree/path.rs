use std::fmt::Write;

use super::{Flavor, Key, NodeId, Tree};

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_' || first == '$')
        && chars.all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '$')
}

fn json_segment(out: &mut String, key: Key<'_>) {
    match key {
        Key::Root => {}
        Key::Index(index) => {
            let _ = write!(out, "[{index}]");
        }
        Key::Name(name) if is_identifier(name) => {
            out.push('.');
            out.push_str(name);
        }
        Key::Name(name) => {
            let quoted = serde_json::to_string(name).unwrap_or_else(|_| format!("\"{name}\""));
            let _ = write!(out, "[{quoted}]");
        }
    }
}

fn xml_position(tree: &Tree, id: NodeId, name: &str) -> Option<usize> {
    let parent = tree.parent(id)?;
    let same: Vec<NodeId> = tree
        .children(parent)
        .filter(|sibling| tree.key(*sibling) == Key::Name(name))
        .collect();
    if same.len() < 2 {
        return None;
    }
    same.iter()
        .position(|sibling| *sibling == id)
        .map(|at| at + 1)
}

fn xml_segment(out: &mut String, tree: &Tree, id: NodeId) {
    let Key::Name(name) = tree.key(id) else {
        return;
    };
    out.push('/');
    out.push_str(name);
    if let Some(position) = xml_position(tree, id, name) {
        let _ = write!(out, "[{position}]");
    }
}

pub(super) fn path(tree: &Tree, id: NodeId) -> String {
    let mut chain = vec![id];
    let mut at = id;
    while let Some(parent) = tree.parent(at) {
        chain.push(parent);
        at = parent;
    }
    let mut out = String::new();
    match tree.flavor() {
        Flavor::Json => {
            out.push('$');
            for node in chain.iter().rev() {
                json_segment(&mut out, tree.key(*node));
            }
        }
        Flavor::Xml => {
            for node in chain.iter().rev() {
                xml_segment(&mut out, tree, *node);
            }
        }
    }
    out
}
