use std::time::{Duration, Instant};

use super::{MAX_NESTING, deepest_in, measure};

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
    measure(text.to_owned()).1.expect("the parse finishes")
}

#[test]
fn the_depth_counts_quotes_lists_inline_marks_tables_and_html() {
    assert_eq!(depth_of("plain\n"), 0);
    assert_eq!(depth_of("> > x\n"), 2);
    assert_eq!(depth_of("- a\n  - b\n"), 2);
    assert_eq!(depth_of("*a **b [c](u)** a*\n"), 3);
    assert_eq!(depth_of("> <div><p>x</p></div>\n"), 3);
    assert_eq!(depth_of("a <b>bold <i>both</i></b> <br> <img src=x/>\n"), 2);
    assert_eq!(depth_of("| a |\n|---|\n| *b* |\n"), 2);
}

#[test]
fn the_reviewer_probes_are_deeper_than_32() {
    for text in [tags("div", 20_000), tags("details", 20_000)] {
        assert!(depth_of(&text) > MAX_NESTING);
    }
    for text in [quote_list(300), wide_list(200)] {
        let started = Instant::now();
        assert!(depth_of(&text) > MAX_NESTING);
        assert!(started.elapsed() < Duration::from_secs(30));
    }
}

#[test]
fn documents_32_levels_deep_are_allowed() {
    for text in [
        quote_list(31),
        wide_list(32),
        tags("div", 32),
        tags("details", 32),
        ">".repeat(32) + " x\n",
    ] {
        assert_eq!(depth_of(&text), MAX_NESTING, "{:?}", &text[..10]);
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
                    let depth = deepest_in(&tree);
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
