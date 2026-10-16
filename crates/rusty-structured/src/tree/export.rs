use super::{Key, NodeId, NodeKind, Tree};
use crate::preview::PREVIEW_CHARS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    Json,
    Yaml,
    Xml,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exported {
    pub text: String,
    pub is_cut: bool,
    pub is_clipped: bool,
    pub is_partial: bool,
}

enum Task {
    Node {
        id: NodeId,
        keyed: bool,
    },
    Siblings {
        at: NodeId,
        keyed: bool,
        before: &'static str,
        between: &'static str,
    },
    Text(&'static str),
    Close(NodeId),
}

struct Writer<'t> {
    tree: &'t Tree,
    syntax: Syntax,
    out: Exported,
    stack: Vec<Task>,
    visited: usize,
}

const YAML_INSIDE: &[char] = &['{', '}', '[', ']', ',', '"', '\''];
const YAML_LEADING: &[char] = &['-', '?', ':', '&', '*', '!', '|', '>', '%', '@', '`', '#'];
const YAML_WORDS: &[&str] = &[
    "null", "Null", "NULL", "~", "true", "True", "TRUE", "false", "False", "FALSE",
];

fn json_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{text}\""))
}

fn yaml_string(text: &str) -> String {
    let is_plain = !text.is_empty()
        && text.trim() == text
        && !text.contains(YAML_INSIDE)
        && !text.starts_with(YAML_LEADING)
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.ends_with(':')
        && !YAML_WORDS.contains(&text)
        && text.parse::<f64>().is_err();
    if is_plain {
        text.to_owned()
    } else {
        json_string(text)
    }
}

fn xml_escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn is_clipped(text: &str) -> bool {
    text.chars().count() > PREVIEW_CHARS
}

fn floor_boundary(text: &str, at: usize) -> usize {
    (0..=at.min(text.len()))
        .rev()
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or(0)
}

impl Writer<'_> {
    fn leaf(&self, id: NodeId) -> String {
        let value = self.tree.value(id);
        match (self.tree.kind(id), self.syntax) {
            (NodeKind::Null, _) => "null".to_owned(),
            (NodeKind::Number, Syntax::Json) if serde_json::from_str::<f64>(value).is_err() => {
                json_string(value)
            }
            (NodeKind::Bool, Syntax::Json) => value.to_ascii_lowercase(),
            (NodeKind::Number | NodeKind::Bool, _) => value.to_owned(),
            (_, Syntax::Json) => json_string(value),
            (_, _) => yaml_string(value),
        }
    }

    fn key(&self, id: NodeId) -> String {
        let name = match self.tree.key(id) {
            Key::Name(name) => name.to_owned(),
            Key::Index(index) => index.to_string(),
            Key::Root => String::new(),
        };
        match self.syntax {
            Syntax::Json => format!("{}:", json_string(&name)),
            Syntax::Yaml | Syntax::Xml => format!("{}: ", yaml_string(&name)),
        }
    }

    fn note_node(&mut self, id: NodeId) {
        self.out.is_partial |= self.tree.lazy(id).is_some();
        self.out.is_clipped |= is_clipped(self.tree.value(id)) || is_clipped(self.tree.attrs(id));
    }

    fn push_children(&mut self, id: NodeId, keyed: bool, between: &'static str) {
        if let Some(first) = self.tree.children(id).next() {
            self.stack.push(Task::Siblings {
                at: first,
                keyed,
                before: "",
                between,
            });
        }
    }

    fn sibling(
        &mut self,
        (at, keyed): (NodeId, bool),
        (before, between): (&'static str, &'static str),
    ) {
        self.out.text.push_str(before);
        if let Some(next) = self.tree.next_sibling(at) {
            self.stack.push(Task::Siblings {
                at: next,
                keyed,
                before: between,
                between,
            });
        }
        self.visited += 1;
        self.stack.push(Task::Node { id: at, keyed });
    }

    fn container(&mut self, id: NodeId) {
        let is_object = self.tree.kind(id) == NodeKind::Object;
        let (open, close) = if is_object { ("{", "}") } else { ("[", "]") };
        self.out.text.push_str(open);
        let between = if self.syntax == Syntax::Json {
            ","
        } else {
            ", "
        };
        self.stack.push(Task::Text(close));
        self.push_children(id, is_object, between);
    }

    fn element(&mut self, id: NodeId) {
        let name = match self.tree.key(id) {
            Key::Name(name) => name,
            Key::Index(_) | Key::Root => "",
        };
        let attrs = self.tree.attrs(id);
        let out = &mut self.out.text;
        out.push('<');
        out.push_str(name);
        if !attrs.is_empty() {
            out.push(' ');
            out.push_str(attrs);
        }
        let value = self.tree.value(id);
        if value.is_empty() && self.tree.children(id).next().is_none() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        out.push_str(&xml_escaped(value));
        self.stack.push(Task::Close(id));
        self.push_children(id, false, "");
    }

    fn node(&mut self, id: NodeId, keyed: bool) {
        self.note_node(id);
        if keyed {
            let key = self.key(id);
            self.out.text.push_str(&key);
        }
        match self.tree.kind(id) {
            NodeKind::Object | NodeKind::Array => self.container(id),
            NodeKind::Element => self.element(id),
            _ => {
                let leaf = self.leaf(id);
                self.out.text.push_str(&leaf);
            }
        }
    }

    fn close_tag(&mut self, id: NodeId) {
        if let Key::Name(name) = self.tree.key(id) {
            self.out.text.push_str("</");
            self.out.text.push_str(name);
            self.out.text.push('>');
        }
    }

    fn run(mut self, cap: usize) -> (Exported, usize) {
        while let Some(task) = self.stack.pop() {
            match task {
                Task::Node { id, keyed } => self.node(id, keyed),
                Task::Siblings {
                    at,
                    keyed,
                    before,
                    between,
                } => self.sibling((at, keyed), (before, between)),
                Task::Text(text) => self.out.text.push_str(text),
                Task::Close(id) => self.close_tag(id),
            }
            if self.out.text.len() > cap {
                break;
            }
        }
        (cut(self.out, cap), self.visited)
    }
}

fn cut(mut out: Exported, cap: usize) -> Exported {
    if out.text.len() > cap {
        let at = floor_boundary(&out.text, cap);
        out.text.truncate(at);
        out.is_cut = true;
    }
    out
}

impl Tree {
    #[must_use]
    pub fn export(&self, id: NodeId, syntax: Syntax, cap: usize) -> Exported {
        self.export_counted(id, syntax, cap).0
    }

    pub(super) fn export_counted(
        &self,
        id: NodeId,
        syntax: Syntax,
        cap: usize,
    ) -> (Exported, usize) {
        let is_leaf = self.children(id).next().is_none() && self.lazy(id).is_none();
        let is_bare = match self.kind(id) {
            NodeKind::Object | NodeKind::Array => false,
            NodeKind::Element => is_leaf,
            _ => true,
        };
        if is_bare {
            let value = self.value(id);
            let text = if self.kind(id) == NodeKind::Null {
                "null"
            } else {
                value
            };
            let out = Exported {
                text: text.to_owned(),
                is_clipped: is_clipped(value),
                ..Exported::default()
            };
            return (cut(out, cap), 1);
        }
        let writer = Writer {
            tree: self,
            syntax,
            out: Exported::default(),
            stack: vec![Task::Node { id, keyed: false }],
            visited: 1,
        };
        writer.run(cap)
    }
}
