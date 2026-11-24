//! Finds the symbols of a text file for the F3 outline popup: definitions, headings and keys.
//! It parses the text with the same tree-sitter grammar the editor highlights with.

use gpui_kit::component::highlighter::LanguageRegistry;
use tree_sitter::{Node, Parser};

/// The most rows the outline lists; a huge JSON file would otherwise build a menu of every key.
pub(super) const MAX_SYMBOLS: usize = 2_000;

/// One outline row: the symbol's name and its 1-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Symbol {
    pub name: String,
    pub line: u64,
}

/// Node kinds that are headings (Markdown) and keyed entries (JSON, TOML, YAML).
const HEADINGS: [&str; 2] = ["atx_heading", "setext_heading"];
const KEYED: [&str; 3] = ["pair", "block_mapping_pair", "flow_pair"];
const TABLES: [&str; 2] = ["table", "table_array_element"];
/// Kind endings of named definitions across grammars (`function_item`, `class_definition`, …).
const DEFINITIONS: [&str; 3] = ["_item", "_definition", "_declaration"];

/// The tree-sitter grammar the editor highlights `language` with, if it has one.
fn grammar(language: &str) -> Option<tree_sitter::Language> {
    LanguageRegistry::singleton()
        .language(language)
        .and_then(|config| config.language)
}

/// Whether `language` has a tree-sitter grammar, so an outline can list anything (a quick lookup).
pub(super) fn has_grammar(language: &str) -> bool {
    grammar(language).is_some()
}

/// Lists the symbols of `text` in document order for the grammar named `language`.
/// A language without a tree-sitter grammar, or text that fails to parse, gives none.
pub(super) fn symbols(language: &str, text: &str) -> Vec<Symbol> {
    let Some(grammar) = grammar(language) else {
        return Vec::new();
    };
    let mut parser = Parser::new();
    if parser.set_language(&grammar).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(text, None) else {
        return Vec::new();
    };
    collect(tree.root_node(), text.as_bytes())
}

/// Walks the tree depth-first with a cursor (no recursion, so deep JSON cannot overflow the stack).
fn collect(root: Node<'_>, src: &[u8]) -> Vec<Symbol> {
    let mut found = Vec::new();
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if let Some(name) = name_of(node, src) {
            let line = u64::try_from(node.start_position().row).unwrap_or(u64::MAX - 1) + 1;
            found.push(Symbol { name, line });
            if found.len() == MAX_SYMBOLS {
                return found;
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return found;
            }
        }
    }
}

/// The outline name of `node`, or `None` when the node is not a symbol.
fn name_of(node: Node<'_>, src: &[u8]) -> Option<String> {
    let kind = node.kind();
    let name = if HEADINGS.contains(&kind) {
        heading(node, src)
    } else if KEYED.contains(&kind) {
        node.child_by_field_name("key").map(|key| text(key, src))
    } else if TABLES.contains(&kind) {
        let mut cursor = node.walk();
        let key = node.named_children(&mut cursor).next();
        key.map(|key| format!("[{}]", text(key, src)))
    } else if kind == "element" {
        element_name(node, src)
    } else if kind == "impl_item" {
        node.child_by_field_name("type")
            .map(|ty| format!("impl {}", text(ty, src)))
    } else if DEFINITIONS.iter().any(|end| kind.ends_with(end)) {
        node.child_by_field_name("name").map(|name| text(name, src))
    } else {
        None
    };
    name.filter(|name| !name.is_empty())
}

/// The text of a Markdown heading without its `#` marks or setext underline.
fn heading(node: Node<'_>, src: &[u8]) -> Option<String> {
    let content = node.child_by_field_name("heading_content").unwrap_or(node);
    let first = text(content, src);
    let line = first.lines().next().unwrap_or_default();
    Some(line.trim().trim_matches('#').trim().to_owned())
}

/// The tag name of an XML element, read from its start or empty-element tag.
fn element_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    let tag = node.named_child(0)?;
    let mut cursor = tag.walk();
    let name = tag
        .named_children(&mut cursor)
        .find(|child| child.kind() == "Name");
    name.map(|name| text(name, src))
}

/// The source text of `node` on one line, without surrounding quotes.
fn text(node: Node<'_>, src: &[u8]) -> String {
    let raw = node.utf8_text(src).unwrap_or_default();
    let line = raw.lines().next().unwrap_or_default().trim();
    line.trim_matches(|c| c == '"' || c == '\'').to_owned()
}
