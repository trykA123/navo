mod elf_build;

use std::sync::atomic::AtomicBool;

use elf_build::{build, firmware, parse};
use rusty_automotive::{Column, Elf, Order, Query, Scope, select};

#[allow(clippy::unwrap_used, reason = "test helper")]
fn names(elf: &Elf, query: &Query) -> Vec<String> {
    select(elf, query, &AtomicBool::new(false))
        .unwrap()
        .into_iter()
        .map(|at| elf.name(&elf.symbols[at as usize]).to_owned())
        .collect()
}

fn order(column: Column, descending: bool) -> Order {
    Order { column, descending }
}

#[test]
fn the_default_order_is_by_address() {
    let elf = parse(&build(&firmware(false, false)));
    assert_eq!(
        names(&elf, &Query::default()),
        [
            "main",
            "Reset_Handler",
            "add(int, int)",
            "counter",
            "state",
            "rx_buffer"
        ]
    );
}

#[test]
fn typing_filters_by_name_ignoring_case() {
    let elf = parse(&build(&firmware(false, false)));
    let query = Query {
        text: "HAND".to_owned(),
        ..Query::default()
    };
    assert_eq!(names(&elf, &query), ["Reset_Handler"]);
}

#[test]
fn sorting_by_size_descending_breaks_ties_by_name() {
    let elf = parse(&build(&firmware(false, false)));
    let query = Query {
        order: order(Column::Size, true),
        ..Query::default()
    };
    assert_eq!(
        names(&elf, &query),
        [
            "rx_buffer",
            "main",
            "Reset_Handler",
            "add(int, int)",
            "counter",
            "state"
        ]
    );
}

#[test]
fn sorting_by_name_folds_case() {
    let elf = parse(&build(&firmware(false, false)));
    let query = Query {
        order: order(Column::Name, false),
        ..Query::default()
    };
    assert_eq!(
        names(&elf, &query),
        [
            "add(int, int)",
            "counter",
            "main",
            "Reset_Handler",
            "rx_buffer",
            "state"
        ]
    );
}

#[test]
fn a_section_scope_keeps_only_its_symbols() {
    let elf = parse(&build(&firmware(false, false)));
    let query = Query {
        scope: Scope::Section(2),
        ..Query::default()
    };
    assert_eq!(names(&elf, &query), ["counter", "state"]);
}

#[test]
fn a_segment_scope_keeps_symbols_inside_its_addresses() {
    let elf = parse(&build(&firmware(false, false)));
    let query = Query {
        scope: Scope::Segment(2),
        ..Query::default()
    };
    assert_eq!(names(&elf, &query), ["rx_buffer"]);
}

#[test]
fn a_cancelled_query_gives_nothing() {
    let elf = parse(&build(&firmware(false, false)));
    assert_eq!(
        select(&elf, &Query::default(), &AtomicBool::new(true)),
        None
    );
}
