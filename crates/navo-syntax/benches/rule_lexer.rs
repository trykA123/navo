//! Callgrind budgets for the navo-syntax rule lexer on generated A2L and UDL pages.

use std::hint::black_box;

use gungraun::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};

mod support;
use support::{Page, a2l_page, lex_page, udl_dense, udl_tokens};

/// Sets an instruction limit for one benchmark case.
fn limit(instructions: u64) -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(Callgrind::default().hard_limits([(EventKind::Ir, instructions)]));
    config
}

// Measures one prepared rule-lexer page.
#[library_benchmark]
#[bench::a2l(args = (a2l_page()), config = limit(3_000_000))]
#[bench::udl_dense(args = (udl_dense()), config = limit(30_000_000))]
fn bench_page(page: Page) -> (usize, usize) {
    black_box(lex_page(&page))
}

// Measures the token cap on one 20 000-keyword UDL row.
#[library_benchmark]
#[bench::udl_tokens(args = (udl_tokens()), config = limit(5_000_000))]
fn bench_cap(page: Page) -> (usize, usize) {
    black_box(lex_page(&page))
}

library_benchmark_group!(name = rule_lexer, benchmarks = [bench_page, bench_cap]);
main!(library_benchmark_groups = rule_lexer);
