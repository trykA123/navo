use std::time::{Duration, Instant};

use super::super::escape::escape_raw;
use super::{MAX_NESTING, measure, shape_of};

fn quote_list(levels: usize) -> String {
    (0..levels)
        .map(|depth| format!("> {}- x\n", "  ".repeat(depth)))
        .collect()
}

fn wide_list(levels: usize) -> String {
    (0..levels)
        .map(|depth| format!("{}-    x\n", " ".repeat(6 * depth)))
        .collect()
}

fn tags(name: &str, levels: usize) -> String {
    format!("<{name}>").repeat(levels) + "x"
}

fn depth_of(text: &str) -> usize {
    measure(text.to_owned())
        .1
        .expect("the parse finishes")
        .depth
}

fn has_raw(text: &str) -> bool {
    measure(text.to_owned())
        .1
        .expect("the parse finishes")
        .has_raw
}

#[test]
fn the_depth_counts_quotes_lists_inline_marks_tables_and_html() {
    assert_eq!(depth_of("plain\n"), 0);
    assert_eq!(depth_of("> > x\n"), 2);
    assert_eq!(depth_of("- a\n  - b\n"), 2);
    assert_eq!(depth_of("*a **b [c](u)** a*\n"), 3);
    assert_eq!(depth_of("| a |\n|---|\n| *b* |\n"), 2);
}

#[test]
fn raw_html_and_math_are_found_and_escaping_removes_them() {
    let probes = [
        "<b></i>".repeat(5_000) + "x" + &"</b>".repeat(5_000),
        "<div></x>".repeat(5_000) + "x",
        "a $".to_owned() + &"<b>".repeat(5_000) + "x" + &"</b>".repeat(5_000) + "$",
        tags("div", 20_000),
        tags("details", 20_000),
        "$$\nx\n$$\n".to_owned(),
        "> <div><p>x</p></div>\n".to_owned(),
    ];
    for probe in probes {
        assert!(has_raw(&probe), "{:?}", &probe[..8]);
        let escaped = escape_raw(&probe);
        let shape = measure(escaped).1.expect("the parse finishes");
        assert!(shape.is_safe(), "{:?} {shape:?}", &probe[..8]);
    }
}

#[test]
fn the_reviewer_probes_are_deeper_than_32() {
    for text in [quote_list(300), wide_list(200)] {
        let started = Instant::now();
        assert!(depth_of(&text) > MAX_NESTING);
        assert!(started.elapsed() < Duration::from_secs(30));
    }
}

#[test]
fn documents_32_levels_deep_are_allowed() {
    for text in [quote_list(31), wide_list(32), ">".repeat(32) + " x\n"] {
        assert_eq!(depth_of(&text), MAX_NESTING, "{:?}", &text[..10]);
    }
}

#[test]
fn the_worst_bracket_shapes_under_the_caps_parse_quickly() {
    let shapes: [String; 3] = [
        (0..20)
            .map(|_| format!("{}x{}\n\n", "![".repeat(1_000), "](u)".repeat(1_000)))
            .collect(),
        format!("[x]: u\n\n{}\n", "[x]".repeat(19_999)),
        (0..20)
            .map(|_| format!("{}x{}\n\n", "[".repeat(1_000), "][x]".repeat(1_000)))
            .collect(),
    ];
    let limit = if cfg!(debug_assertions) { 30 } else { 2 };
    for shape in shapes {
        let started = Instant::now();
        assert!(depth_of(&shape) <= MAX_NESTING);
        assert!(started.elapsed() < Duration::from_secs(limit));
    }
}

fn nested_quotes(levels: usize) -> markdown::mdast::Node {
    use markdown::mdast::{Blockquote, Node};
    (0..levels).fold(
        Node::Text(markdown::mdast::Text {
            value: "x".to_owned(),
            position: None,
        }),
        |inner, _| {
            Node::Blockquote(Blockquote {
                children: vec![inner],
                position: None,
            })
        },
    )
}

#[test]
fn the_walk_has_no_recursion_and_fits_a_1_mib_stack() {
    let big = std::thread::Builder::new().stack_size(256 * 1024 * 1024);
    let depth = big
        .spawn(|| {
            let tree = nested_quotes(10_000);
            let small = std::thread::Builder::new().stack_size(1024 * 1024);
            let (tree, depth) = small
                .spawn(move || {
                    let depth = shape_of(&tree).depth;
                    (tree, depth)
                })
                .expect("a 1 MiB thread starts")
                .join()
                .expect("the walk does not overflow");
            drop(tree);
            depth
        })
        .expect("a big thread starts")
        .join()
        .expect("the tree drops");
    assert_eq!(depth, 10_000);
}
