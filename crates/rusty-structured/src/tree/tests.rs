use crate::json::{parse_json, parse_jsonc};
use crate::yaml::parse_yaml;
use crate::{Key, NodeId, NodeKind, Outline, Step, Tree};

const STORE: &str = r#"{
  "store": {
    "book": [
      {"title": "Sayings", "price": 8.95},
      {"title": "Sword", "price": 12}
    ],
    "open": true,
    "owner": null
  },
  "weird key": "x"
}"#;

fn reachable(tree: &Tree) -> usize {
    let mut stack: Vec<NodeId> = tree.root().into_iter().collect();
    let mut count = 0;
    while let Some(at) = stack.pop() {
        count += 1;
        stack.extend(tree.children(at));
    }
    count
}

fn find(tree: &Tree, path: &str) -> NodeId {
    tree.ids()
        .find(|id| tree.path(*id) == path)
        .unwrap_or_else(|| panic!("no node at {path}"))
}

#[test]
fn json_tree_has_every_value_with_its_path_kind_and_preview() {
    let tree = parse_json(STORE.as_bytes()).unwrap();
    assert_eq!(reachable(&tree), 12);
    let root = tree.root().unwrap();
    assert_eq!(
        (tree.kind(root), tree.child_count(root)),
        (NodeKind::Object, 2)
    );
    assert_eq!(tree.path(root), "$");
    let price = find(&tree, "$.store.book[1].price");
    assert_eq!(
        (tree.kind(price), tree.value(price)),
        (NodeKind::Number, "12")
    );
    let book = find(&tree, "$.store.book");
    assert_eq!(
        (tree.kind(book), tree.child_count(book)),
        (NodeKind::Array, 2)
    );
    let title = find(&tree, "$.store.book[0].title");
    assert_eq!(
        (tree.kind(title), tree.value(title)),
        (NodeKind::String, "Sayings")
    );
    let owner = find(&tree, "$.store.owner");
    assert_eq!(tree.kind(owner), NodeKind::Null);
    let weird = find(&tree, "$[\"weird key\"]");
    assert_eq!(tree.key(weird), Key::Name("weird key"));
    let keys: Vec<_> = tree.children(root).map(|id| tree.key(id)).collect();
    assert_eq!(keys, [Key::Name("store"), Key::Name("weird key")]);
}

#[test]
fn long_strings_are_clipped_to_one_line() {
    let long = format!("[\"{}\\nsecond\"]", "a".repeat(400));
    let tree = parse_json(long.as_bytes()).unwrap();
    let item = tree.children(tree.root().unwrap()).next().unwrap();
    let value = tree.value(item);
    assert!(value.ends_with('…'), "{value}");
    assert!(!value.contains('\n'));
    assert!(value.chars().count() <= 161);
}

#[test]
fn malformed_json_reports_line_and_column() {
    let error = parse_json(b"{\n  \"a\": 1,\n  \"b\": ]\n}").unwrap_err();
    assert_eq!((error.line, error.column), (3, 8), "{error}");
    assert!(!error.message.contains("line"), "{}", error.message);
}

#[test]
fn truncated_or_deep_input_errors_without_panicking() {
    for end in 0..STORE.len() {
        let _ = parse_json(&STORE.as_bytes()[..end]);
        let _ = parse_jsonc(&STORE.as_bytes()[..end]);
        let _ = parse_yaml(&STORE.as_bytes()[..end]);
    }
    assert!(parse_json("[".repeat(100_000).as_bytes()).is_err());
    assert!(parse_json(b"").is_err());
}

#[test]
fn jsonc_skips_comments_and_trailing_commas_and_keeps_positions() {
    let text = "{\n  // note\n  \"a\": [1, 2,], /* more */\n  \"b\": \"//kept\",\n}";
    let tree = parse_jsonc(text.as_bytes()).unwrap();
    assert_eq!(reachable(&tree), 5);
    assert_eq!(tree.value(find(&tree, "$.b")), "//kept");
    let error = parse_jsonc(b"{\n  // note\n  \"a\": }").unwrap_err();
    assert_eq!(error.line, 3);
    assert!(parse_json(text.as_bytes()).is_err());
}

const YAML: &str = "name: demo
items:
  - id: 1
    tags: [a, b]
  - id: 2
empty: ~
flag: true
quoted: \"12\"
";

#[test]
fn yaml_tree_has_every_value_with_its_path_and_kind() {
    let tree = parse_yaml(YAML.as_bytes()).unwrap();
    assert_eq!(reachable(&tree), 13);
    let root = tree.root().unwrap();
    assert_eq!(tree.path(root), "$");
    assert_eq!(tree.kind(root), NodeKind::Object);
    let tag = find(&tree, "$.items[0].tags[1]");
    assert_eq!((tree.kind(tag), tree.value(tag)), (NodeKind::String, "b"));
    assert_eq!(tree.kind(find(&tree, "$.items[1].id")), NodeKind::Number);
    assert_eq!(tree.kind(find(&tree, "$.empty")), NodeKind::Null);
    assert_eq!(tree.kind(find(&tree, "$.flag")), NodeKind::Bool);
    assert_eq!(tree.kind(find(&tree, "$.quoted")), NodeKind::String);
}

#[test]
fn several_yaml_documents_become_a_list() {
    let tree = parse_yaml(b"a: 1\n---\nb: 2\n").unwrap();
    let root = tree.root().unwrap();
    assert_eq!(
        (tree.kind(root), tree.child_count(root)),
        (NodeKind::Array, 2)
    );
    assert_eq!(tree.value(find(&tree, "$[1].b")), "2");
}

#[test]
fn malformed_yaml_reports_line_and_column() {
    let error = parse_yaml(b"a: 1\nb: [1, 2\nc: 3\n").unwrap_err();
    assert!(error.line >= 2, "{error}");
    assert!(error.column >= 1, "{error}");
    let error = parse_yaml(b"a: 1\n  b: 2\n").unwrap_err();
    assert_eq!(error.line, 2, "{error}");
}

fn shown(tree: &Tree, outline: &Outline) -> Vec<String> {
    outline
        .rows()
        .iter()
        .map(|row| tree.path(row.node))
        .collect()
}

#[test]
fn right_expands_and_left_collapses_or_goes_to_the_parent() {
    let tree = parse_json(STORE.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    assert_eq!(shown(&tree, &outline), ["$", "$.store", "$[\"weird key\"]"]);
    outline.move_by(1);
    assert_eq!(outline.right(&tree), Step::Done);
    assert_eq!(outline.rows().len(), 6);
    outline.right(&tree);
    assert_eq!(tree.path(outline.selected().unwrap()), "$.store.book");
    outline.left(&tree);
    assert_eq!(tree.path(outline.selected().unwrap()), "$.store");
    outline.left(&tree);
    assert_eq!(shown(&tree, &outline), ["$", "$.store", "$[\"weird key\"]"]);
    outline.left(&tree);
    assert_eq!(outline.cursor(), 0);
}

#[test]
fn star_expands_the_subtree_and_minus_collapses_all() {
    let tree = parse_json(STORE.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    outline.move_by(1);
    outline.expand_subtree(&tree);
    assert_eq!(outline.rows().len(), 12);
    outline.move_by(4);
    assert_eq!(
        tree.path(outline.selected().unwrap()),
        "$.store.book[0].price"
    );
    outline.collapse_all(&tree);
    assert_eq!(outline.rows().len(), 3);
    assert_eq!(tree.path(outline.selected().unwrap()), "$.store");
}

#[test]
fn a_filter_shows_matching_keys_with_their_parents_and_clearing_reveals_the_pick() {
    let tree = parse_json(STORE.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    outline.set_filter(&tree, "PRICE");
    assert_eq!(
        shown(&tree, &outline),
        [
            "$",
            "$.store",
            "$.store.book",
            "$.store.book[0]",
            "$.store.book[0].price",
            "$.store.book[1]",
            "$.store.book[1].price",
        ]
    );
    assert_eq!(
        tree.path(outline.selected().unwrap()),
        "$.store.book[0].price"
    );
    outline.move_by(2);
    outline.set_filter(&tree, "");
    assert_eq!(
        tree.path(outline.selected().unwrap()),
        "$.store.book[1].price"
    );
    assert!(outline.rows().len() > 3);
}

#[test]
fn moves_stay_inside_the_rows() {
    let tree = parse_json(STORE.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    outline.move_by(-5);
    assert_eq!(outline.cursor(), 0);
    outline.move_by(50);
    assert_eq!(outline.cursor(), 2);
}
