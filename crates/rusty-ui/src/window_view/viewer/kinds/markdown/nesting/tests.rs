use std::time::{Duration, Instant};

use super::fails_fast;

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

#[test]
fn long_emphasis_runs_and_wide_container_prefixes_fail_fast() {
    let cases = [
        format!("{}x{}", "*".repeat(1_001), "*".repeat(1_001)),
        format!("x{}x", "_".repeat(1_001)),
        format!("x{}x", "~".repeat(1_001)),
        "*a ".repeat(50_000),
        quote_list(3_000),
        wide_list(2_000),
        format!("{}x\n", ">".repeat(257)),
        format!("{}- x\n", " ".repeat(300)),
        format!("{}x\n", "[".repeat(1_001)),
        "[a] ".repeat(20_001),
        (0..30)
            .map(|_| format!("{}x{}\n\n", "[".repeat(900), "]".repeat(900)))
            .collect(),
    ];
    for text in cases {
        let started = Instant::now();
        assert!(fails_fast(&text), "{:?}", &text[..12]);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}

#[test]
fn ordinary_documents_and_code_pass_to_the_parser() {
    let fine = [
        "# Notes\n\n- one\n  - two\n\n> quoted **bold** [link](x)\n".to_owned(),
        quote_list(31),
        wide_list(32),
        "***\n---\n- - -\n".repeat(5_000),
        "snake_case_name and a * b\n".repeat(20_000),
        "*a* ".repeat(4_000),
        format!("```\n{}```\n", quote_list(3_000)),
        format!("{}let x = 1;\n", " ".repeat(400)),
        "[a] ".repeat(20_000),
        format!("{}x\n\n{}x\n", "[".repeat(1_000), "[".repeat(1_000)),
        format!("```\n{}\n```\n", "[".repeat(30_000)),
    ];
    for text in fine {
        assert!(!fails_fast(&text), "{:?}", &text[..12]);
    }
}

#[test]
fn the_scan_of_hostile_input_fits_a_1_mib_stack() {
    let text = format!("{}x{}", "*".repeat(50_000), "*".repeat(50_000));
    let worker = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || fails_fast(&text))
        .expect("a 1 MiB thread starts");
    assert!(worker.join().expect("the scan does not overflow"));
}
