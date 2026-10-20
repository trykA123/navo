#![allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]

use std::time::Instant;

use proptest::prelude::*;
use rusty_automotive::space::{Item, Part, Rect, Target, Tile, layout};

const BOUNDS: Rect = Rect {
    x: 10.0,
    y: 20.0,
    w: 800.0,
    h: 500.0,
};

fn item(id: u32, value: u64) -> Item {
    Item {
        id,
        value,
        parts: Vec::new(),
    }
}

fn items(values: &[u64]) -> Vec<Item> {
    values
        .iter()
        .enumerate()
        .map(|(at, &value)| item(at as u32, value))
        .collect()
}

fn overlap(a: &Rect, b: &Rect) -> f64 {
    let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
}

fn aspect(rect: &Rect) -> f64 {
    rect.w.max(rect.h) / rect.w.min(rect.h)
}

fn assert_inside(rect: &Rect, bounds: &Rect) {
    let slack = 1e-6;
    assert!(
        rect.x >= bounds.x - slack && rect.y >= bounds.y - slack,
        "{rect:?}"
    );
    assert!(rect.x + rect.w <= bounds.x + bounds.w + slack, "{rect:?}");
    assert!(rect.y + rect.h <= bounds.y + bounds.h + slack, "{rect:?}");
}

fn assert_clean(tiles: &[Tile], bounds: &Rect) {
    for (at, tile) in tiles.iter().enumerate() {
        assert_inside(&tile.rect, bounds);
        for other in &tiles[at + 1..] {
            assert!(
                overlap(&tile.rect, &other.rect) < 1e-6,
                "{tile:?} {other:?}"
            );
        }
    }
}

fn parts(values: &[u64]) -> Vec<Part> {
    values
        .iter()
        .enumerate()
        .map(|(at, &value)| Part {
            id: 100 + at as u32,
            value,
        })
        .collect()
}

#[test]
fn space_treemap_areas_follow_weights() {
    let values = [600_u64, 300, 200, 150, 100, 50, 7, 3];
    let tiles = layout(&items(&values), BOUNDS);
    let total: u64 = values.iter().sum();
    assert_eq!(tiles.len(), values.len());
    for tile in &tiles {
        let want = tile.value as f64 / total as f64 * BOUNDS.area();
        assert!((tile.rect.area() - want).abs() <= want * 1e-9, "{tile:?}");
    }
    assert!((tiles.iter().map(|tile| tile.rect.area()).sum::<f64>() - BOUNDS.area()).abs() < 1e-6);
}

#[test]
fn space_treemap_tiles_do_not_overlap_and_stay_inside() {
    let values: Vec<u64> = (1..=22).map(|n| n * n * 13).collect();
    assert_clean(&layout(&items(&values), BOUNDS), &BOUNDS);
}

#[test]
fn space_treemap_matches_the_published_squarify_example() {
    let bounds = Rect::new(0.0, 0.0, 6.0, 4.0);
    let tiles = layout(&items(&[6, 6, 4, 3, 2, 2, 1]), bounds);
    assert_eq!(tiles[0].rect, Rect::new(0.0, 0.0, 3.0, 2.0));
    assert_eq!(tiles[1].rect, Rect::new(0.0, 2.0, 3.0, 2.0));
    let worst = tiles
        .iter()
        .map(|tile| aspect(&tile.rect))
        .fold(0.0, f64::max);
    assert!(worst < 2.8, "worst aspect {worst}");
    assert_clean(&tiles, &bounds);
}

#[test]
fn space_treemap_equal_weights_make_squares() {
    let bounds = Rect::new(0.0, 0.0, 400.0, 400.0);
    let tiles = layout(&items(&[5; 16]), bounds);
    for tile in &tiles {
        assert!((tile.rect.w - 100.0).abs() < 1e-9 && (tile.rect.h - 100.0).abs() < 1e-9);
    }
    let wide = layout(&items(&[5; 22]), BOUNDS);
    assert!(wide.iter().all(|tile| aspect(&tile.rect) < 2.0));
}

#[test]
fn space_treemap_cuts_after_22_groups() {
    let values: Vec<u64> = (1..=30).collect();
    let tiles = layout(&items(&values), BOUNDS);
    assert_eq!(tiles.len(), 23);
    let more = tiles.last().unwrap();
    assert_eq!(more.target, Target::More { count: 8 });
    assert_eq!(more.value, (1..=8).sum::<u64>());
    let named: Vec<u64> = tiles[..22].iter().map(|tile| tile.value).collect();
    assert_eq!(named, (9..=30).rev().collect::<Vec<u64>>());
}

#[test]
fn space_treemap_exactly_22_groups_have_no_more_tile() {
    let tiles = layout(&items(&(1..=22).collect::<Vec<u64>>()), BOUNDS);
    assert_eq!(tiles.len(), 22);
    assert!(
        tiles
            .iter()
            .all(|tile| matches!(tile.target, Target::Item(_)))
    );
}

#[test]
fn space_treemap_drops_zero_weights() {
    let tiles = layout(&items(&[0, 5, 0, 5]), BOUNDS);
    assert_eq!(tiles.len(), 2);
    assert!(layout(&items(&[0, 0]), BOUNDS).is_empty());
    assert!(layout(&[], BOUNDS).is_empty());
}

#[test]
fn space_treemap_tiny_weights_keep_a_proportional_sliver() {
    let tiles = layout(&items(&[1_000_000_000, 1]), BOUNDS);
    assert_eq!(tiles.len(), 2);
    let sliver = &tiles[1];
    assert!(sliver.rect.area() > 0.0);
    assert!((sliver.rect.area() - BOUNDS.area() / 1_000_000_001.0).abs() < 1e-9);
    assert_clean(&tiles, &BOUNDS);
}

#[test]
fn space_treemap_order_is_by_size_then_input_order() {
    let tiles = layout(&items(&[5, 9, 5, 7]), BOUNDS);
    let ids: Vec<Target> = tiles.iter().map(|tile| tile.target).collect();
    assert_eq!(
        ids,
        [
            Target::Item(1),
            Target::Item(3),
            Target::Item(0),
            Target::Item(2)
        ]
    );
    assert_eq!(tiles, layout(&items(&[5, 9, 5, 7]), BOUNDS));
}

#[test]
fn space_treemap_degenerate_bounds_give_empty_tiles() {
    let tiles = layout(&items(&[3, 4]), Rect::new(0.0, 0.0, 0.0, 100.0));
    assert_eq!(tiles.len(), 2);
    assert!(
        tiles
            .iter()
            .all(|tile| tile.rect.area() == 0.0 && !tile.text)
    );
}

#[test]
fn space_treemap_text_needs_more_than_64_by_34() {
    let wide = layout(&items(&[1]), Rect::new(0.0, 0.0, 65.0, 35.0));
    assert!(wide[0].text);
    for (w, h) in [(64.0, 35.0), (65.0, 34.0), (64.0, 34.0)] {
        assert!(
            !layout(&items(&[1]), Rect::new(0.0, 0.0, w, h))[0].text,
            "{w}x{h}"
        );
    }
}

#[test]
fn space_treemap_inner_tiles_need_more_than_150_by_110() {
    let mut big = item(1, 100);
    big.parts = parts(&[60, 30, 10]);
    let tiles = layout(
        std::slice::from_ref(&big),
        Rect::new(0.0, 0.0, 151.0, 111.0),
    );
    assert_eq!(tiles[0].inner.len(), 3);
    for (w, h) in [
        (150.0, 111.0),
        (151.0, 110.0),
        (150.0, 110.0),
        (100.0, 100.0),
    ] {
        let tiles = layout(std::slice::from_ref(&big), Rect::new(0.0, 0.0, w, h));
        assert!(tiles[0].inner.is_empty(), "{w}x{h}");
    }
}

#[test]
fn space_treemap_inner_tiles_fill_the_area_below_the_header() {
    let mut big = item(1, 100);
    big.parts = parts(&[50, 30, 20]);
    let rect = Rect::new(5.0, 6.0, 300.0, 200.0);
    let tile = &layout(&[big], rect)[0];
    let area = Rect::new(5.0, 6.0 + 18.0, 300.0, 200.0 - 18.0 - 4.0);
    assert_clean(&tile.inner, &area);
    let covered: f64 = tile.inner.iter().map(|inner| inner.rect.area()).sum();
    assert!((covered - area.area()).abs() < 1e-6);
    assert_eq!(tile.inner[0].target, Target::Item(100));
    assert!(tile.inner.iter().all(|inner| inner.inner.is_empty()));
}

#[test]
fn space_treemap_inner_tiles_cut_after_14_parts() {
    let mut big = item(1, 1000);
    big.parts = parts(&(1..=20).collect::<Vec<u64>>());
    let tile = &layout(&[big], Rect::new(0.0, 0.0, 600.0, 400.0))[0];
    assert_eq!(tile.inner.len(), 15);
    assert_eq!(tile.inner[14].target, Target::More { count: 6 });
    assert_eq!(tile.inner[14].value, (1..=6).sum::<u64>());
}

#[test]
fn space_treemap_more_tile_has_no_inner_tiles() {
    let mut list = items(&(1..=25).collect::<Vec<u64>>());
    for entry in &mut list {
        entry.parts = parts(&[entry.value]);
    }
    let tiles = layout(&list, Rect::new(0.0, 0.0, 3000.0, 2000.0));
    let more = tiles.last().unwrap();
    assert!(matches!(more.target, Target::More { .. }));
    assert!(more.inner.is_empty());
    assert!(tiles[..22].iter().all(|tile| !tile.inner.is_empty()));
}

proptest! {
    #[test]
    fn space_treemap_always_partitions_the_rectangle(
        values in proptest::collection::vec(0_u64..1_000_000, 0..60),
        w in 1.0_f64..3000.0,
        h in 1.0_f64..2000.0,
    ) {
        let bounds = Rect::new(3.0, 4.0, w, h);
        let tiles = layout(&items(&values), bounds);
        let live = values.iter().filter(|&&value| value > 0).count().min(23);
        prop_assert_eq!(tiles.len(), live);
        if !tiles.is_empty() {
            let area: f64 = tiles.iter().map(|tile| tile.rect.area()).sum();
            prop_assert!((area - bounds.area()).abs() <= bounds.area() * 1e-9);
        }
        assert_clean(&tiles, &bounds);
    }
}

#[test]
#[cfg_attr(debug_assertions, ignore = "the budget is for release builds")]
fn space_treemap_layout_of_22_by_14_stays_in_budget() {
    let mut list = items(&(1..=40).map(|n| n * 977 % 5000 + 1).collect::<Vec<u64>>());
    for entry in &mut list {
        entry.parts = parts(&(1..=30).map(|n| n * 31 % 700 + 1).collect::<Vec<u64>>());
    }
    let bounds = Rect::new(0.0, 0.0, 1400.0, 800.0);
    let started = Instant::now();
    let tiles = layout(&list, bounds);
    let elapsed = started.elapsed();
    assert_eq!(tiles.len(), 23);
    assert!(elapsed.as_micros() < 1000, "treemap took {elapsed:?}");
}
