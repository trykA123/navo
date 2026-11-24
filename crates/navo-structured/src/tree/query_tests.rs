//! Tests for the JSONPath-like tree query parser and selector.

use std::collections::HashSet;

use super::query::Query;
use super::{Flavor, Key, NewNode, NodeId, NodeKind, Tree};
use crate::json::parse_json;
use crate::xml::parse_xml;

fn fixture() -> Tree {
    parse_json(br#"{"a":[{"b":1},{"b":2,"c":{"b":3}}],"d":"x"}"#).unwrap()
}

fn query(text: &str) -> Query {
    Query::parse(text).unwrap()
}

fn values(tree: &Tree, ids: &[NodeId]) -> Vec<String> {
    ids.iter().map(|id| tree.value(*id).to_owned()).collect()
}

#[test]
fn root_selects_the_root() {
    let tree = fixture();
    assert_eq!(tree.select(&query("$")), vec![tree.root().unwrap()]);
}

#[test]
fn child_index_child() {
    let tree = fixture();
    assert_eq!(values(&tree, &tree.select(&query("$.a[1].b"))), ["2"]);
}

#[test]
fn wildcard_then_child() {
    let tree = fixture();
    assert_eq!(values(&tree, &tree.select(&query("$.a.*.b"))), ["1", "2"]);
}

#[test]
fn descendants_in_document_order() {
    let tree = fixture();
    assert_eq!(values(&tree, &tree.select(&query("$..b"))), ["1", "2", "3"]);
}

#[test]
fn missing_name_is_empty() {
    let tree = fixture();
    assert!(tree.select(&query("$.zz")).is_empty());
}

#[test]
fn star_index() {
    let tree = fixture();
    assert_eq!(tree.select(&query("$.a[*]")).len(), 2);
}

#[test]
fn all_descendants() {
    let tree = fixture();
    let ids = tree.select(&query("$..*"));
    assert_eq!(ids.len(), tree.len() - 1);
    assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), ids.len());
}

#[test]
fn no_duplicates() {
    let tree = fixture();
    assert_eq!(tree.select(&query("$..a..b")).len(), 3);
}

#[test]
fn xml_children_by_name() {
    let tree = parse_xml(b"<r><i>1</i><i>2</i><j/></r>").unwrap();
    assert_eq!(tree.select(&query("$.i")).len(), 2);
    let j = tree.select(&query("$[2]"));
    assert_eq!(j.len(), 1);
    assert_eq!(tree.key(j[0]), Key::Name("j"));
}

fn chain(depth: usize) -> Tree {
    let mut tree = Tree::new(Flavor::Json);
    let mut parent = None;
    for level in 0..=depth {
        let key = if level == 0 {
            Key::Root
        } else {
            Key::Name("a")
        };
        let node = NewNode {
            kind: NodeKind::Object,
            key,
            value: "",
            attrs: "",
        };
        parent = tree.push(parent, node);
    }
    tree
}

#[test]
fn deep_chain_descendant_steps_stay_linear() {
    let tree = chain(400);
    let (ids, visited) = tree.select_counted(&query("$..*..*..*"));
    assert_eq!(ids.len(), 398);
    assert!(visited < 400 * 400, "{visited} ids visited");
    assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), ids.len());
}

#[test]
fn errors_have_columns() {
    let cases = [
        ("a.b", "a query starts with $", 1),
        ("$.", "expected a name after .", 3),
        ("$[x]", "expected [n] or [*]", 2),
        ("$.a[1", "expected [n] or [*]", 4),
        ("$ .a", "unexpected character", 2),
    ];
    for (text, message, column) in cases {
        let error = Query::parse(text).unwrap_err();
        assert_eq!((error.line, error.column), (1, column), "{text}");
        assert_eq!(error.message, message, "{text}");
    }
}
