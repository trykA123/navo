use crate::json::parse_json;
use crate::tree::NewNode;
use crate::{Flavor, Key, Lazy, NodeKind, Outline, Span, Tree};

const NESTED: &str = r#"{"a": {"b": {"c": {"d": {"e": {"f": {"g": {"h": {"i": {"j": {"k": 1}}}}}}}}}}, "z": [1, 2]}"#;

fn shown(tree: &Tree, outline: &Outline) -> Vec<String> {
    outline
        .rows()
        .iter()
        .map(|row| tree.path(row.node))
        .collect()
}

fn selected(tree: &Tree, outline: &Outline) -> String {
    tree.path(outline.selected().unwrap())
}

fn go_to(tree: &Tree, outline: &mut Outline, path: &str) {
    let row = shown(tree, outline)
        .iter()
        .position(|shown| shown == path)
        .unwrap_or_else(|| panic!("{path} is not shown"));
    outline.move_to(row);
}

#[test]
fn depth_one_shows_only_the_children_of_the_root() {
    let tree = parse_json(NESTED.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    outline.expand_all(&tree);
    outline.fold_to_depth(&tree, 1);
    assert_eq!(shown(&tree, &outline), ["$", "$.a", "$.z"]);
}

#[test]
fn depth_three_opens_every_node_above_three_and_closes_the_rest() {
    let tree = parse_json(NESTED.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    outline.expand_all(&tree);
    outline.fold_to_depth(&tree, 3);
    assert_eq!(
        shown(&tree, &outline),
        ["$", "$.a", "$.a.b", "$.a.b.c", "$.z", "$.z[0]", "$.z[1]"]
    );
}

#[test]
fn depth_nine_shows_nine_levels_from_a_collapsed_tree() {
    let tree = parse_json(NESTED.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    outline.fold_to_depth(&tree, 9);
    let rows = shown(&tree, &outline);
    assert_eq!(rows.len(), 13);
    assert_eq!(rows[9], "$.a.b.c.d.e.f.g.h.i");
    assert!(!outline.is_open(outline.rows()[9].node));
}

#[test]
fn folding_to_a_depth_keeps_a_visible_cursor_and_lifts_a_hidden_one() {
    let tree = parse_json(NESTED.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    outline.expand_all(&tree);
    go_to(&tree, &mut outline, "$.z[1]");
    outline.fold_to_depth(&tree, 3);
    assert_eq!(selected(&tree, &outline), "$.z[1]");
    outline.expand_all(&tree);
    go_to(&tree, &mut outline, "$.a.b.c.d.e.f.g.h.i.j.k");
    outline.fold_to_depth(&tree, 3);
    assert_eq!(selected(&tree, &outline), "$.a.b.c");
}

#[test]
fn closing_a_subtree_closes_everything_under_it() {
    let tree = parse_json(NESTED.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    outline.expand_all(&tree);
    go_to(&tree, &mut outline, "$.a");
    outline.close_subtree(&tree);
    let open_z = ["$.z", "$.z[0]", "$.z[1]"];
    assert_eq!(
        shown(&tree, &outline),
        [&["$", "$.a"][..], &open_z].concat()
    );
    assert_eq!(selected(&tree, &outline), "$.a");
    outline.right(&tree);
    assert_eq!(
        shown(&tree, &outline),
        [&["$", "$.a", "$.a.b"][..], &open_z].concat()
    );
}

#[test]
fn closing_the_root_subtree_keeps_the_root_open() {
    let tree = parse_json(NESTED.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    outline.expand_all(&tree);
    outline.close_subtree(&tree);
    assert_eq!(shown(&tree, &outline), ["$", "$.a", "$.z"]);
    assert_eq!(outline.cursor(), 0);
}

#[test]
fn expanding_all_opens_every_loaded_node_and_keeps_the_cursor() {
    let tree = parse_json(NESTED.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    go_to(&tree, &mut outline, "$.z");
    outline.expand_all(&tree);
    assert_eq!(outline.rows().len(), 15);
    assert_eq!(selected(&tree, &outline), "$.z");
}

fn lazy_tree() -> Tree {
    let mut tree = Tree::new(Flavor::Json);
    let node = |kind, key| NewNode {
        kind,
        key,
        value: "",
        attrs: "",
    };
    let root = tree.push(None, node(NodeKind::Object, Key::Root)).unwrap();
    let lazy = tree
        .push(Some(root), node(NodeKind::Object, Key::Name("lazy")))
        .unwrap();
    let span = Span { start: 0, end: 10 };
    tree.set_lazy(lazy, Lazy { span, count: 3 });
    let loaded = tree
        .push(Some(root), node(NodeKind::Object, Key::Name("loaded")))
        .unwrap();
    tree.push(Some(loaded), node(NodeKind::Null, Key::Name("leaf")));
    tree
}

#[test]
fn expanding_all_and_folding_to_a_depth_never_load_a_lazy_node() {
    let tree = lazy_tree();
    let mut outline = Outline::new(&tree);
    outline.expand_all(&tree);
    assert_eq!(
        shown(&tree, &outline),
        ["$", "$.lazy", "$.loaded", "$.loaded.leaf"]
    );
    outline.fold_to_depth(&tree, 9);
    assert_eq!(outline.rows().len(), 4);
    assert!(!outline.is_open(outline.rows()[1].node));
}

#[test]
fn folds_do_nothing_while_a_filter_shows() {
    let tree = parse_json(NESTED.as_bytes()).unwrap();
    let mut outline = Outline::new(&tree);
    outline.set_filter(&tree, "k");
    let rows = shown(&tree, &outline);
    outline.expand_all(&tree);
    outline.fold_to_depth(&tree, 1);
    outline.close_subtree(&tree);
    assert_eq!(shown(&tree, &outline), rows);
}
