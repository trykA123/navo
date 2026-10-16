use std::time::{Duration, Instant};

use rusty_structured::markdown::{MAX_BLOCKS, MAX_SOURCE, MAX_SPANS, Refusal, parse};

const SMALL_STACK: usize = 1024 * 1024;
const MAX_HEAP: usize = 64 * 1024 * 1024;

fn fill(unit: &str) -> String {
    unit.repeat((MAX_SOURCE - 1024) / unit.len())
}

fn tags(name: &str, levels: usize) -> String {
    format!("<{name}>").repeat(levels) + "x"
}

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

fn backtick_runs() -> String {
    (1..2_000)
        .map(|len| "`".repeat(len % 60 + 1) + " x ")
        .collect()
}

fn footnotes(count: usize) -> String {
    let uses: String = (0..count).map(|at| format!("a[^n{at}] ")).collect();
    let notes: String = (0..count)
        .map(|at| format!("[^n{at}]: note {at}\n\n"))
        .collect();
    uses + "\n\n" + &notes
}

fn references(count: usize) -> String {
    let uses: String = (0..count).map(|at| format!("[r{at}] ")).collect();
    let defs: String = (0..count)
        .map(|at| format!("[r{at}]: https://example.com/{at}\n"))
        .collect();
    uses + "\n\n" + &defs
}

fn probes() -> Vec<(&'static str, String)> {
    let n = 200_000;
    vec![
        (
            "padded table",
            "|a".repeat(100) + "|\n" + &"|-".repeat(100) + "|\n" + &fill("x\n"),
        ),
        ("a lines", fill("a\n")),
        ("angle lines", fill("<\n")),
        ("list items", fill("- a\n")),
        ("list items cr", fill("- a\r")),
        ("backtick runs", backtick_runs()),
        ("backtick fill", fill("`` a ` b ``` c ")),
        ("div tags", tags("div", 20_000)),
        ("details tags", tags("details", 20_000)),
        (
            "mismatched tags",
            "<b></i>".repeat(n) + "x" + &"</b>".repeat(n),
        ),
        (
            "math tags",
            "a $".to_owned() + &"<b>".repeat(n) + "x" + &"</b>".repeat(n) + "$",
        ),
        ("math runs", fill("$x$ $$y$$ ")),
        ("display math", fill("$$\nx\n$$\n")),
        ("nested quotes", ">".repeat(100_000) + " x\n"),
        ("quote list", quote_list(1_400)),
        ("wide list", wide_list(1_100)),
        ("open brackets", fill("[")),
        ("open images", fill("![")),
        ("inline code", "`a` ".repeat(70_000)),
        ("autolinks", fill("<https://example.com/a> ")),
        ("footnotes", footnotes(5_000)),
        ("references", references(5_000)),
        (
            "stars",
            "*".repeat(50_000) + "x" + &"*".repeat(50_000) + "\n",
        ),
        ("emphasis openers", "*a ".repeat(50_000)),
        (
            "deep list",
            (0..1_500)
                .map(|depth| format!("{}- x\n", "  ".repeat(depth)))
                .collect(),
        ),
    ]
}

fn limit() -> Duration {
    if cfg!(debug_assertions) {
        Duration::from_secs(20)
    } else {
        Duration::from_secs(1)
    }
}

fn check(name: &str, source: &str) {
    assert!(source.len() <= MAX_SOURCE, "{name} fits the size cap");
    let started = Instant::now();
    let parsed = parse(source);
    let took = started.elapsed();
    assert!(took < limit(), "{name} took {took:?}");
    match parsed {
        Ok(doc) => {
            assert!(doc.blocks().len() <= MAX_BLOCKS, "{name}");
            assert!(doc.spans_total() <= MAX_SPANS, "{name}");
            assert!(doc.heap_bytes() <= MAX_HEAP, "{name}: {}", doc.heap_bytes());
        }
        Err(refusal) => assert!(
            matches!(refusal, Refusal::TooManyBlocks | Refusal::TooManySpans),
            "{name}: {refusal:?}"
        ),
    }
}

#[test]
fn every_review_probe_parses_quickly_on_a_small_stack_within_the_caps() {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(|| {
            for (name, source) in probes() {
                check(name, &source);
            }
        })
        .expect("the probe thread starts")
        .join()
        .expect("no probe overflows the stack");
}

#[test]
fn html_and_math_probes_keep_their_literal_text() {
    let doc = parse(&tags("div", 20_000)).expect("renders");
    let text: String = doc.blocks().iter().map(|block| doc.text(block)).collect();
    assert!(text.starts_with("<div><div>"));
    let doc = parse("costs $5 and $6, or <b>bold</b>\n").expect("renders");
    let block = doc.blocks()[0];
    assert_eq!(doc.text(&block), "costs $5 and $6, or <b>bold</b>");
}
