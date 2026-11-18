//! Callgrind budgets for streaming XML, JSON and Log page lexing.

use std::hint::black_box;

use gungraun::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};

mod support;
use support::{
    Page, arxml_typical, json_dense, lex_line, lex_page, line_over_max, log_1mib, log_dense,
    log_typical, xml_dense,
};

/// Sets an instruction limit for one benchmark case.
fn limit(instructions: u64) -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(Callgrind::default().hard_limits([(EventKind::Ir, instructions)]));
    config
}

// Measures streaming lexing of one prepared page.
#[library_benchmark]
#[bench::arxml_typical(args = (arxml_typical()), config = limit(2_000_000))]
#[bench::xml_dense(args = (xml_dense(120, false)), config = limit(25_000_000))]
#[bench::json_dense(args = (json_dense()), config = limit(30_000_000))]
#[bench::xml_capped(args = (xml_dense(300, true)), config = limit(30_000_000))]
#[bench::log_typical(args = (log_typical()), config = limit(2_000_000))]
#[bench::log_dense(args = (log_dense()), config = limit(25_000_000))]
fn bench_page(page: Page) -> (usize, usize) {
    black_box(lex_page(&page))
}

// Measures a full 1 MiB catch-up run over dense Log rows.
#[library_benchmark]
#[bench::log_1mib(args = (log_1mib()), config = limit(60_000_000))]
fn bench_catch_up(page: Page) -> (usize, usize) {
    black_box(lex_page(&page))
}

// Measures the editor line cap rejecting a line over MAX_LINE.
#[library_benchmark]
#[bench::line_over_max(args = (line_over_max()), config = limit(50_000))]
fn bench_cap(line: String) -> (usize, usize) {
    black_box(lex_line(&line))
}

library_benchmark_group!(
    name = line_lexers,
    benchmarks = [bench_page, bench_catch_up, bench_cap]
);
main!(library_benchmark_groups = line_lexers);
