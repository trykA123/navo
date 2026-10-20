#![allow(clippy::unwrap_used, reason = "test helpers")]

mod elf_build;

use std::sync::atomic::AtomicBool;

use elf_build::{Named, Spec, absolute, build, firmware, func, named, object, parse};
use object::elf as raw;
use rusty_automotive::{Column, Elf, Order, Query, Scope, estimated_size, select};

fn names(elf: &Elf, query: &Query) -> Vec<String> {
    select(elf, query, &AtomicBool::new(false))
        .unwrap()
        .rows
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

fn local(symbol: Named) -> Named {
    Named {
        bind: raw::STB_LOCAL,
        kind: raw::STT_NOTYPE,
        ..symbol
    }
}

fn weak(symbol: Named) -> Named {
    Named {
        bind: raw::STB_WEAK,
        ..symbol
    }
}

fn noisy(extra: Vec<Named>) -> Spec {
    let mut spec = firmware(false, false);
    spec.symbols.as_mut().unwrap().extend(extra);
    spec
}

fn selection(elf: &Elf, query: &Query) -> rusty_automotive::Selection {
    select(elf, query, &AtomicBool::new(false)).unwrap()
}

fn index_of(elf: &Elf, name: &str) -> usize {
    named(elf, name).unwrap().0
}

#[test]
fn noise_is_hidden_and_counted() {
    let elf = parse(&build(&noisy(vec![
        local(func(".L42", 0x0800_0110, 0, 0)),
        local(func(".L43", 0x0800_0120, 0, 0)),
        absolute("__stack_top", 0x2000_8000),
        local(func("empty_data", 0x2000_0010, 0, 1)),
    ])));
    let shown = selection(&elf, &Query::default());
    assert_eq!(shown.hidden, 4);
    assert_eq!(shown.rows.len(), 6);
}

#[test]
fn file_symbols_never_reach_the_table() {
    let elf = parse(&build(&firmware(false, false)));
    assert!(named(&elf, "main.c").is_none());
    assert_eq!(selection(&elf, &Query::default()).hidden, 0);
}

#[test]
fn an_estimated_global_is_shown() {
    let elf = parse(&build(&noisy(vec![func("asm_entry", 0x0800_0160, 0, 0)])));
    let shown = names(&elf, &Query::default());
    assert!(shown.contains(&"asm_entry".to_owned()));
    assert_eq!(selection(&elf, &Query::default()).hidden, 0);
}

#[test]
fn a_zero_size_weak_object_is_estimated_too() {
    let elf = parse(&build(&noisy(vec![weak(object(
        "table",
        0x2000_0010,
        0,
        1,
    ))])));
    assert_eq!(estimated_size(&elf, index_of(&elf, "table")), Some(0x30));
    assert!(names(&elf, &Query::default()).contains(&"table".to_owned()));
}

#[test]
fn the_estimate_runs_to_the_next_symbol() {
    let elf = parse(&build(&noisy(vec![
        func("asm_entry", 0x0800_0160, 0, 0),
        func("after", 0x0800_0180, 4, 0),
    ])));
    assert_eq!(
        estimated_size(&elf, index_of(&elf, "asm_entry")),
        Some(0x20)
    );
}

#[test]
fn the_estimate_stops_at_the_section_end() {
    let elf = parse(&build(&noisy(vec![func("tail", 0x0800_03f0, 0, 0)])));
    assert_eq!(estimated_size(&elf, index_of(&elf, "tail")), Some(0x10));
}

#[test]
fn a_symbol_past_its_section_end_has_no_estimate() {
    let elf = parse(&build(&noisy(vec![func("stray", 0x0800_0400, 0, 0)])));
    assert_eq!(estimated_size(&elf, index_of(&elf, "stray")), None);
    assert_eq!(selection(&elf, &Query::default()).hidden, 1);
}

#[test]
fn sized_and_local_symbols_have_no_estimate() {
    let elf = parse(&build(&noisy(vec![local(func(".L7", 0x0800_0170, 0, 0))])));
    assert_eq!(estimated_size(&elf, index_of(&elf, "main")), None);
    assert_eq!(estimated_size(&elf, index_of(&elf, ".L7")), None);
    assert_eq!(estimated_size(&elf, 9999), None);
}

#[test]
fn a_local_zero_size_label_is_hidden() {
    let elf = parse(&build(&noisy(vec![local(func(".L42", 0x0800_0110, 0, 0))])));
    assert!(!names(&elf, &Query::default()).contains(&".L42".to_owned()));
}

#[test]
fn a_label_does_not_give_a_sized_function_a_new_row() {
    let elf = parse(&build(&noisy(vec![local(func(".L42", 0x0800_0110, 0, 0))])));
    assert_eq!(selection(&elf, &Query::default()).rows.len(), 6);
}

#[test]
fn turning_the_filter_off_shows_every_symbol() {
    let elf = parse(&build(&noisy(vec![
        local(func(".L42", 0x0800_0110, 0, 0)),
        absolute("__stack_top", 0x2000_8000),
    ])));
    let query = Query {
        hide_noise: false,
        ..Query::default()
    };
    let shown = selection(&elf, &query);
    assert_eq!(shown.rows.len(), elf.symbols.len());
    assert_eq!(shown.hidden, 0);
}

#[test]
fn the_hidden_count_follows_the_typed_text() {
    let elf = parse(&build(&noisy(vec![
        local(func(".L42", 0x0800_0110, 0, 0)),
        absolute("__stack_top", 0x2000_8000),
    ])));
    let query = Query {
        text: ".L".to_owned(),
        ..Query::default()
    };
    let shown = selection(&elf, &query);
    assert_eq!(shown.hidden, 1);
    assert!(shown.rows.is_empty());
}

#[test]
fn merged_sections_full_of_labels_still_filter_well() {
    let labels = (0..88)
        .map(|at| local(func(&format!(".L{at}"), 0x0800_0200 + at, 0, 0)))
        .collect();
    let elf = parse(&build(&noisy(labels)));
    let shown = selection(&elf, &Query::default());
    assert_eq!(shown.hidden, 88);
    assert_eq!(shown.rows.len(), 6);
}
