use std::time::{Duration, Instant};

use super::{MAX_NESTING, is_too_deep};

fn nested(prefix: &str, levels: usize, body: &str) -> String {
    format!("{}{body}\n", prefix.repeat(levels))
}

fn list(levels: usize, indent: &str) -> String {
    (0..levels)
        .map(|depth| format!("{}- x\n", indent.repeat(depth)))
        .collect()
}

#[test]
fn nesting_over_32_levels_is_too_deep() {
    let over = MAX_NESTING + 1;
    let cases = [
        nested(">", over, "x"),
        nested("> ", over, "x"),
        nested("[", over, "x"),
        nested("- ", over, "x"),
        nested("1. ", over, "x"),
        nested("- > ", over / 2 + 1, "x"),
        list(over, "  "),
        list(over, "    "),
        list(over, "\t"),
        format!("{}x{}", "*".repeat(1_001), "*".repeat(1_001)),
        format!("{}x", "_".repeat(1_001)),
        format!("x{}x", "~".repeat(1_001)),
    ];
    for text in cases {
        assert!(is_too_deep(&text), "{:?}", &text[..12]);
    }
}

#[test]
fn exactly_32_levels_and_ordinary_documents_are_rendered() {
    let fine = [
        "# Notes\n\n- one\n  - two\n    - three\n\n> quoted **bold** [link](x)\n".to_owned(),
        nested(">", MAX_NESTING, "x"),
        list(MAX_NESTING, "  "),
        list(MAX_NESTING, "    "),
        format!("{}\n\n{}", "[".repeat(30), "[".repeat(30)),
        "***\n---\n- - -\n".repeat(5_000),
        "snake_case_name and a * b\n".repeat(20_000),
    ];
    for text in fine {
        assert!(!is_too_deep(&text), "{:?}", &text[..12]);
    }
}

#[test]
fn code_blocks_never_count_as_nesting() {
    let deep = nested(">", 200, "x") + &list(200, "  ") + &nested("[", 200, "x");
    let fenced = [
        format!("```rust\n{deep}```\n"),
        format!("~~~~ text\n{deep}~~~~\n"),
        format!("> ```\n{deep}> ```\n"),
    ];
    for text in fenced {
        assert!(!is_too_deep(&text), "{:?}", &text[..12]);
    }
    let indented: String = deep.lines().map(|line| format!("    {line}\n")).collect();
    assert!(!is_too_deep(&format!("Code:\n\n{indented}")));
    assert!(is_too_deep(&format!("```\ncode\n```\n{deep}")));
}

#[test]
fn over_10_000_emphasis_runs_are_too_deep_and_found_quickly() {
    let text = "*a ".repeat(50_000);
    let started = Instant::now();
    assert!(is_too_deep(&text));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!is_too_deep(&"*a* ".repeat(4_000)));
}

#[test]
fn the_scan_of_hostile_input_fits_a_1_mib_stack() {
    let text = format!("{}x{}", "*".repeat(50_000), "*".repeat(50_000));
    let worker = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || is_too_deep(&text))
        .expect("a 1 MiB thread starts");
    assert!(worker.join().expect("the scan does not overflow"));
}
