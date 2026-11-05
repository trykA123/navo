use super::{DensePages, EMPTY, PAGE, PAGE_BYTES, Page, Placed};
use crate::heap_probe::{Probe, fail_allocations_from};

const BUDGET_SLACK: usize = 256 * 1024;

fn bytes(pages: &DensePages) -> usize {
    let (directory, held) = pages.memory_parts();
    directory + held
}

/// Length, held triples and memory parts.
type Snapshot = (usize, Vec<(u64, u16, u32)>, (usize, usize));

fn snapshot(pages: &DensePages) -> Snapshot {
    (pages.len(), pages.iter().collect(), pages.memory_parts())
}

#[test]
fn page_and_directory_slot_have_the_planned_sizes() {
    assert_eq!(size_of::<Option<Page>>(), 40);
    assert!(size_of::<Page>() <= 48);
    assert_eq!(PAGE_BYTES, 6 * 1024);
}

#[test]
fn consecutive_records_cost_at_most_six_point_one_bytes() {
    let probe = Probe::start();
    let mut pages = DensePages::default();
    for record in 0..1_000_000u32 {
        let placed = pages.try_insert(u64::from(record), 1, record).unwrap();
        assert_eq!(placed, Placed::Inserted);
    }
    pages.shrink_to_fit();
    assert_eq!(pages.len(), 1_000_000);
    assert_eq!(pages.get(999_999, 1), Some(999_999));
    let held = probe.read().retained;
    assert_eq!(
        held,
        bytes(&pages) as isize,
        "memory_parts matches the heap"
    );
    let per_record = bytes(&pages) as f64 / 1_000_000.0;
    assert!(per_record <= 6.1, "{per_record} B per record");
}

#[test]
fn a_lone_record_far_away_is_refused() {
    let mut pages = DensePages::default();
    assert_eq!(pages.try_insert(1 << 31, 1, 4).unwrap(), Placed::Refused);
    assert_eq!(snapshot(&pages), (0, Vec::new(), (0, 0)));
    assert_eq!(pages.try_insert(1 << 32, 1, 4).unwrap(), Placed::Refused);
    assert_eq!(pages.get(1 << 32, 1), None);
}

#[test]
fn records_spread_to_two_to_the_31_stay_within_the_budget() {
    let mut pages = DensePages::default();
    let step = (1u64 << 31) / 100_000;
    for n in 0..100_000u32 {
        let record = u64::from(n) * step;
        if pages.try_insert(record, 1, n).unwrap() == Placed::Inserted {
            assert_eq!(pages.get(record, 1), Some(n));
        }
        assert!(bytes(&pages) <= 24 * pages.len() + BUDGET_SLACK);
    }
    assert!(pages.len() > 0);
    assert!(bytes(&pages) <= 24 * 100_000 + BUDGET_SLACK);
}

#[test]
fn a_page_is_freed_when_its_last_record_goes() {
    let mut pages = DensePages::default();
    pages.try_insert(5, 1, 0).unwrap();
    pages.try_insert(6, 1, 1).unwrap();
    assert_eq!(pages.memory_parts().1, PAGE_BYTES);
    assert_eq!(pages.remove(5, 2), None);
    assert_eq!(pages.remove(5, 1), Some(0));
    assert_eq!(pages.memory_parts().1, PAGE_BYTES);
    assert_eq!(pages.remove(6, 1), Some(1));
    assert_eq!(pages.memory_parts().1, 0);
    assert_eq!((pages.len(), pages.get(6, 1)), (0, None));
}

#[test]
fn another_seq_in_a_held_slot_is_refused() {
    let mut pages = DensePages::default();
    assert_eq!(pages.try_insert(7, 1, 3).unwrap(), Placed::Inserted);
    let before = snapshot(&pages);
    assert_eq!(pages.try_insert(7, 2, 4).unwrap(), Placed::Refused);
    assert_eq!(snapshot(&pages), before);
    assert_eq!((pages.get(7, 1), pages.get(7, 2)), (Some(3), None));
    assert_eq!(pages.try_insert(7, 1, 8).unwrap(), Placed::Updated(3));
    assert_eq!((pages.len(), pages.get(7, 1)), (1, Some(8)));
}

#[test]
fn an_empty_id_is_refused_whether_or_not_the_key_is_held() {
    let mut pages = DensePages::default();
    assert_eq!(pages.try_insert(9, 1, EMPTY).unwrap(), Placed::Refused);
    assert_eq!(snapshot(&pages), (0, Vec::new(), (0, 0)));
    pages.try_insert(9, 1, 3).unwrap();
    let before = snapshot(&pages);
    assert_eq!(pages.try_insert(9, 1, EMPTY).unwrap(), Placed::Refused);
    assert_eq!(snapshot(&pages), before);
    assert_eq!(pages.get(9, 1), Some(3));
}

fn three_pages() -> DensePages {
    let mut pages = DensePages::default();
    for (n, record) in [1u64, 2, 1500, 3000, 3001].into_iter().enumerate() {
        pages.try_insert(record, 7, n as u32).unwrap();
    }
    pages
}

#[test]
fn remap_and_retain_never_allocate_when_no_id_maps_to_empty() {
    let mut pages = three_pages();
    let probe = Probe::start();
    let moved = pages.remap(|id| (id != 1).then_some(id + 10));
    pages.retain(|record, _, _| record != 3000);
    assert_eq!(probe.read().peak, 0);
    assert!(moved.is_empty());
    drop(probe);
    let left: Vec<_> = pages.iter().collect();
    assert_eq!(left, [(1, 7, 10), (1500, 7, 12), (3001, 7, 14)]);
}

#[test]
fn remap_hands_back_and_removes_pairs_that_become_empty() {
    let mut pages = three_pages();
    let moved = pages.remap(|id| Some(if id == 2 { EMPTY } else { id }));
    assert_eq!(moved, [(1500, 7, EMPTY)]);
    assert_eq!((pages.len(), pages.get(1500, 7)), (4, None));
    assert_eq!(pages.memory_parts().1, 2 * PAGE_BYTES);
}

#[test]
fn iter_runs_in_record_order_and_shrink_drops_trailing_slots() {
    let mut pages = DensePages::default();
    pages.try_insert(3 * PAGE as u64 + 4, 2, 0).unwrap();
    pages.try_insert(8, 1, 1).unwrap();
    let all: Vec<_> = pages.iter().collect();
    assert_eq!(all, [(8, 1, 1), (3076, 2, 0)]);
    pages.remove(3076, 2);
    pages.shrink_to_fit();
    assert_eq!(pages.memory_parts(), (40, PAGE_BYTES));
    pages.clear();
    assert_eq!(
        (pages.len(), pages.get(8, 1), pages.memory_parts().1),
        (0, None, 0)
    );
}

#[test]
fn a_failed_page_allocation_changes_nothing() {
    let mut pages = DensePages::default();
    pages.try_insert(1, 1, 0).unwrap();
    let before = snapshot(&pages);
    let refused = {
        let _fail = fail_allocations_from(2048);
        pages.try_insert(5000, 1, 1)
    };
    assert!(refused.is_err());
    assert_eq!(snapshot(&pages), before);
}

#[test]
fn a_failed_directory_reservation_after_the_page_changes_nothing() {
    let mut pages = DensePages::default();
    let refused = {
        let _fail = fail_allocations_from(8192);
        pages.try_insert(1 << 20, 1, 0)
    };
    assert!(refused.is_err());
    assert_eq!(snapshot(&pages), (0, Vec::new(), (0, 0)));
    assert_eq!(pages.try_insert(1 << 20, 1, 0).unwrap(), Placed::Inserted);
    assert_eq!(pages.memory_parts(), (1025 * 40, PAGE_BYTES));
}
