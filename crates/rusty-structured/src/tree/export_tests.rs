use super::export::{Exported, Syntax};
use crate::json::parse_json;
use crate::xml::parse_xml;
use crate::yaml::parse_yaml;
use crate::{NodeId, Tree};

const MIB: usize = 1024 * 1024;

fn at(tree: &Tree, path: &str) -> NodeId {
    tree.ids()
        .find(|id| tree.path(*id) == path)
        .unwrap_or_else(|| panic!("no node at {path}"))
}

fn text(exported: &Exported) -> &str {
    &exported.text
}

#[test]
fn a_json_leaf_copies_its_bare_value() {
    let tree = parse_json(br#"{"name": "shop \"one\"", "qty": 12, "open": null}"#).unwrap();
    let name = tree.export(at(&tree, "$.name"), Syntax::Json, MIB);
    assert_eq!(text(&name), "shop \"one\"");
    assert_eq!(
        text(&tree.export(at(&tree, "$.qty"), Syntax::Json, MIB)),
        "12"
    );
    assert_eq!(
        text(&tree.export(at(&tree, "$.open"), Syntax::Json, MIB)),
        "null"
    );
}

#[test]
fn a_json_object_copies_as_compact_json() {
    let source = br#"{"store": {"book": [{"t": "a\"b", "p": 8.5}, true], "n": null}}"#;
    let tree = parse_json(source).unwrap();
    let store = tree.export(at(&tree, "$.store"), Syntax::Json, MIB);
    assert_eq!(
        text(&store),
        r#"{"book":[{"t":"a\"b","p":8.5},true],"n":null}"#
    );
    assert!(!store.is_cut && !store.is_clipped && !store.is_partial);
    let empty = parse_json(br#"{"a": {}, "b": []}"#).unwrap();
    let root = empty.root().unwrap();
    assert_eq!(
        text(&empty.export(root, Syntax::Json, MIB)),
        r#"{"a":{},"b":[]}"#
    );
}

#[test]
fn a_yaml_mapping_copies_as_flow_yaml() {
    let source =
        "name: demo\nitems:\n  - id: 1\n    tags: [a, 'b c']\n  - id: 2\nempty: ~\nodd: 'x: y'\n";
    let tree = parse_yaml(source.as_bytes()).unwrap();
    let root = tree.root().unwrap();
    assert_eq!(
        text(&tree.export(root, Syntax::Yaml, MIB)),
        r#"{name: demo, items: [{id: 1, tags: [a, b c]}, {id: 2}], empty: null, odd: "x: y"}"#
    );
}

#[test]
fn an_xml_element_copies_as_xml_and_a_text_only_element_as_its_text() {
    let source =
        br#"<shop id="1"><name>A &amp; B</name><items><item/><item>x</item></items></shop>"#;
    let tree = parse_xml(source).unwrap();
    let root = tree.root().unwrap();
    assert_eq!(
        text(&tree.export(root, Syntax::Xml, MIB)),
        r#"<shop id="1"><name>A &amp; B</name><items><item/><item>x</item></items></shop>"#
    );
    assert_eq!(
        text(&tree.export(at(&tree, "/shop/name"), Syntax::Xml, MIB)),
        "A & B"
    );
}

#[test]
fn a_large_value_is_cut_at_the_cap_on_a_character_boundary() {
    let items: Vec<String> = (0..2000).map(|at| format!("\"é{at}\"")).collect();
    let source = format!("[{}]", items.join(","));
    let tree = parse_json(source.as_bytes()).unwrap();
    let exported = tree.export(tree.root().unwrap(), Syntax::Json, 1001);
    assert!(exported.is_cut);
    assert!(exported.text.len() <= 1001);
    assert!(exported.text.starts_with("[\"é0\",\"é1\""));
}

#[test]
fn a_value_clipped_by_the_tree_is_reported() {
    let source = format!("{{\"long\": \"{}\", \"short\": \"x\"}}", "a".repeat(400));
    let tree = parse_json(source.as_bytes()).unwrap();
    assert!(
        tree.export(tree.root().unwrap(), Syntax::Json, MIB)
            .is_clipped
    );
    assert!(
        tree.export(at(&tree, "$.long"), Syntax::Json, MIB)
            .is_clipped
    );
    assert!(
        !tree
            .export(at(&tree, "$.short"), Syntax::Json, MIB)
            .is_clipped
    );
}

#[test]
fn deep_nesting_exports_without_recursion() {
    let depth = 100_000;
    let source = format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth));
    let tree = parse_xml(source.as_bytes()).unwrap();
    let exported = tree.export(tree.root().unwrap(), Syntax::Xml, 64 * MIB);
    let inner = "<a>".repeat(depth - 1) + "<a/>" + &"</a>".repeat(depth - 1);
    assert_eq!(exported.text, inner);
}
