#![allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]

use std::time::Instant;

use proptest::prelude::*;
use rusty_automotive::space::{
    Block, Kind, NameRef, Owner, OwnerRef, Owners, Region, RegionSource, Scale, ScaleMode, cells,
    inner,
};

fn region(total: Option<u64>, blocks: &[(u64, u64, Kind)]) -> Region {
    let mut region = Region {
        name: "flash".to_owned(),
        start: Some(0),
        start_estimated: false,
        total,
        chip: None,
        reserved: None,
        used: 0,
        by_kind: [0; Kind::COUNT],
        blocks: Vec::new(),
        source: RegionSource::Mdf,
    };
    for (at, &(start, size, kind)) in blocks.iter().enumerate() {
        region.used += size;
        region.blocks.push(Block {
            start,
            size,
            kind,
            owner: OwnerRef::ElfSection(at),
        });
    }
    region
}

fn compressed(region: &Region) -> Scale {
    Scale::new(region, ScaleMode::Compressed, None)
}

fn true_scale(region: &Region) -> Scale {
    Scale::new(region, ScaleMode::True, None)
}

#[test]
fn space_strip_linker_fill_keeps_its_own_kind() {
    let region = region(
        Some(1000),
        &[
            (0, 100, Kind::Code),
            (100, 100, Kind::Fill),
            (200, 1, Kind::Fill),
        ],
    );
    let found = cells(&region, &true_scale(&region), 100);
    let kinds: Vec<Kind> = found.iter().map(|cell| cell.kind).collect();
    assert_eq!(kinds, [Kind::Code, Kind::Fill, Kind::Fill]);
}

#[test]
fn space_strip_cells_follow_address_order() {
    let region = region(
        Some(1000),
        &[
            (0, 100, Kind::Code),
            (100, 100, Kind::Rodata),
            (200, 100, Kind::Data),
            (300, 100, Kind::Bss),
        ],
    );
    let found = cells(&region, &true_scale(&region), 100);
    let kinds: Vec<Kind> = found.iter().map(|cell| cell.kind).collect();
    assert_eq!(kinds, [Kind::Code, Kind::Rodata, Kind::Data, Kind::Bss]);
    let edges: Vec<(u32, u32)> = found.iter().map(|cell| (cell.x0, cell.x1)).collect();
    assert_eq!(edges, [(0, 10), (10, 20), (20, 30), (30, 40)]);
    assert_eq!(found[2].blocks, 2..3);
    assert_eq!(found[2].bytes, 100);
}

#[test]
fn space_strip_gap_is_free_space_without_a_cell() {
    let region = region(Some(1000), &[(0, 100, Kind::Code), (600, 100, Kind::Data)]);
    let found = cells(&region, &true_scale(&region), 100);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].x1, 10);
    assert_eq!(found[1].x0, 60);
    assert!(found[1].x1 <= 70);
}

#[test]
fn space_strip_gap_over_four_percent_is_compressed() {
    let region = region(Some(1000), &[(0, 250, Kind::Code), (750, 250, Kind::Data)]);
    let scale = compressed(&region);
    assert_eq!(scale.breaks().len(), 1);
    assert_eq!(scale.compressed_bytes(), 500);
    let item = scale.breaks()[0];
    assert_eq!((item.start, item.size), (250, 500));
    assert!((item.x0 - 250.0 / 515.0).abs() < 1e-9);
    assert!((item.x1 - 265.0 / 515.0).abs() < 1e-9);
    assert!((scale.x(750) - 265.0 / 515.0).abs() < 1e-9);
    assert!((scale.x(1000) - 1.0).abs() < 1e-9);
}

#[test]
fn space_strip_true_scale_keeps_every_byte_linear() {
    let region = region(Some(1000), &[(0, 250, Kind::Code), (750, 250, Kind::Data)]);
    let scale = true_scale(&region);
    assert!(scale.breaks().is_empty());
    assert!((scale.x(500) - 0.5).abs() < 1e-9);
    assert_eq!(scale.offset(0.75), 750);
}

#[test]
fn space_strip_short_gap_stays_linear() {
    let region = region(Some(1000), &[(0, 480, Kind::Code), (500, 500, Kind::Data)]);
    assert!(compressed(&region).breaks().is_empty());
}

#[test]
fn space_strip_free_tail_is_compressed_when_long() {
    let region = region(Some(1000), &[(0, 100, Kind::Code)]);
    let scale = compressed(&region);
    assert_eq!(scale.breaks().len(), 1);
    assert_eq!(scale.breaks()[0].start, 100);
    assert_eq!(scale.breaks()[0].size, 900);
}

#[test]
fn space_strip_cluster_region_has_no_breaks() {
    let region = region(None, &[(0, 100, Kind::Code), (900, 100, Kind::Data)]);
    let scale = compressed(&region);
    assert!(scale.breaks().is_empty());
    assert_eq!(scale.domain(), 0..1000);
    assert!((scale.x(500) - 0.5).abs() < 1e-9);
}

#[test]
fn space_strip_tiny_blocks_keep_one_pixel() {
    let region = region(
        Some(1 << 20),
        &[
            (0, 1, Kind::Code),
            (500_000, 3, Kind::Data),
            ((1 << 20) - 1, 1, Kind::Bss),
        ],
    );
    let found = cells(&region, &true_scale(&region), 1600);
    assert_eq!(found.len(), 3);
    assert!(found.iter().all(|cell| cell.x1 - cell.x0 >= 1));
    assert_eq!(found[0].x0, 0);
    assert_eq!(found[2].x1, 1600);
}

#[test]
fn space_strip_sub_pixel_blocks_merge_with_dominant_kind() {
    let mut blocks = Vec::new();
    for at in 0..100_u64 {
        let kind = if at % 10 < 3 { Kind::Code } else { Kind::Data };
        let size = if kind == Kind::Code { 120 } else { 40 };
        blocks.push((at * 1000, size, kind));
    }
    let region = region(Some(100_000), &blocks);
    let found = cells(&region, &true_scale(&region), 10);
    assert_eq!(found.len(), 10);
    for cell in &found {
        assert_eq!(cell.blocks.len(), 10);
        assert_eq!(cell.bytes, 3 * 120 + 7 * 40);
        assert_eq!(cell.kind, Kind::Code);
    }
}

#[test]
fn space_strip_merged_cells_never_exceed_pixel_count() {
    let blocks: Vec<_> = (0..5000_u64).map(|at| (at * 20, 7, Kind::Code)).collect();
    let region = region(Some(100_000), &blocks);
    for width in [1_u32, 7, 100, 1600] {
        let found = cells(&region, &true_scale(&region), width);
        assert!(found.len() <= width as usize, "width {width}");
        assert!(found.windows(2).all(|pair| pair[0].x1 <= pair[1].x0));
        assert!(found.iter().all(|cell| cell.x1 <= width));
        assert_eq!(found.iter().map(|cell| cell.bytes).sum::<u64>(), 5000 * 7);
    }
}

#[test]
fn space_strip_zero_width_gives_no_cells() {
    let region = region(Some(100), &[(0, 50, Kind::Code)]);
    assert!(cells(&region, &true_scale(&region), 0).is_empty());
}

#[test]
fn space_strip_zoom_view_clips_blocks() {
    let region = region(
        Some(1000),
        &[
            (0, 300, Kind::Code),
            (300, 400, Kind::Data),
            (700, 300, Kind::Bss),
        ],
    );
    let scale = Scale::new(&region, ScaleMode::Compressed, Some(250..750));
    assert_eq!(scale.domain(), 250..750);
    let found = cells(&region, &scale, 100);
    let kinds: Vec<Kind> = found.iter().map(|cell| cell.kind).collect();
    assert_eq!(kinds, [Kind::Code, Kind::Data, Kind::Bss]);
    assert_eq!(found[0].bytes, 50);
    assert_eq!(found[2].bytes, 50);
    assert_eq!((found[0].x0, found[0].x1), (0, 10));
    assert_eq!((found[2].x0, found[2].x1), (90, 100));
    assert_eq!(found[0].blocks, 0..1);
}

#[test]
fn space_strip_zoom_skips_blocks_outside_the_view() {
    let region = region(
        Some(1000),
        &[
            (0, 100, Kind::Code),
            (400, 100, Kind::Data),
            (900, 100, Kind::Bss),
        ],
    );
    let scale = Scale::new(&region, ScaleMode::True, Some(400..500));
    let found = cells(&region, &scale, 50);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].blocks, 1..2);
    assert_eq!((found[0].x0, found[0].x1), (0, 50));
}

#[test]
fn space_strip_empty_region_is_harmless() {
    let empty = region(Some(1000), &[]);
    let scale = compressed(&empty);
    assert!(scale.breaks().is_empty());
    assert!(cells(&empty, &scale, 100).is_empty());
    let none = region(None, &[]);
    assert_eq!(
        Scale::new(&none, ScaleMode::Compressed, None).offset(0.5),
        0
    );
}

proptest! {
    #[test]
    fn space_strip_offset_inverts_x(
        gap in 1_u64..1_000_000,
        head in 1_u64..100_000,
        tail in 1_u64..100_000,
        probe in 0_u64..1_000_000,
        mode in prop_oneof![Just(ScaleMode::Compressed), Just(ScaleMode::True)],
    ) {
        let total = head + gap + tail;
        let region = region(Some(total), &[(0, head, Kind::Code), (head + gap, tail, Kind::Data)]);
        let scale = Scale::new(&region, mode, None);
        let offset = probe % total;
        let back = scale.offset(scale.x(offset));
        prop_assert!(back.abs_diff(offset) <= 1, "{offset} -> {back}");
    }

    #[test]
    fn space_strip_cells_stay_ordered(
        sizes in proptest::collection::vec((1_u64..5000, 0_u64..5000), 1..200),
        width in 1_u32..2000,
    ) {
        let mut at = 0;
        let mut blocks = Vec::new();
        for (size, gap) in sizes {
            blocks.push((at, size, Kind::Rodata));
            at += size + gap;
        }
        let region = region(Some(at), &blocks);
        for scale in [compressed(&region), true_scale(&region)] {
            let found = cells(&region, &scale, width);
            prop_assert!(found.len() <= width as usize);
            prop_assert!(found.windows(2).all(|pair| pair[0].x1 <= pair[1].x0));
            prop_assert!(found.iter().all(|cell| cell.x0 < cell.x1 && cell.x1 <= width));
            prop_assert_eq!(found.iter().map(|cell| cell.bytes).sum::<u64>(), region.used);
        }
    }
}

#[test]
#[cfg_attr(debug_assertions, ignore = "the budget is for release builds")]
fn space_strip_cells_for_1600_px_stay_in_budget() {
    let blocks: Vec<_> = (0..5000_u64)
        .map(|at| (at * 1000, 300 + at % 400, Kind::Code))
        .collect();
    let region = region(Some(5_000_000), &blocks);
    let started = Instant::now();
    let scale = compressed(&region);
    let found = cells(&region, &scale, 1600);
    let elapsed = started.elapsed();
    assert!(found.len() <= 1600);
    assert!(
        elapsed.as_millis() < 5,
        "strip cells took {elapsed:?} for 5000 blocks"
    );
}

fn owners_of(items: &[(u64, u64, Kind)]) -> Owners {
    let mut owners = Owners::default();
    owners.items = items
        .iter()
        .enumerate()
        .map(|(at, &(start, size, kind))| Owner {
            start,
            size,
            name: NameRef::Symbol(at as u32),
            kind,
            section: 0,
            object: None,
            source_file: None,
            estimated: false,
            image: false,
        })
        .collect();
    owners
}

fn block(start: u64, size: u64) -> Block {
    Block {
        start,
        size,
        kind: Kind::Code,
        owner: OwnerRef::ElfSection(0),
    }
}

#[test]
fn space_strip_inner_cells_split_a_wide_block_by_owner() {
    let owners = owners_of(&[
        (0, 400, Kind::Code),
        (400, 300, Kind::Rodata),
        (700, 300, Kind::Code),
    ]);
    let whole = block(0, 1000);
    let region = region(Some(1000), &[(0, 1000, Kind::Code)]);
    let found = inner(&owners, &whole, &true_scale(&region), 200);
    let edges: Vec<(u32, u32)> = found.iter().map(|cell| (cell.x0, cell.x1)).collect();
    assert_eq!(edges, [(0, 80), (80, 140), (140, 200)]);
    let kinds: Vec<Kind> = found.iter().map(|cell| cell.kind).collect();
    assert_eq!(kinds, [Kind::Code, Kind::Rodata, Kind::Code]);
    assert_eq!(found[1].blocks, 1..2);
    assert_eq!(found[1].bytes, 300);
}

#[test]
fn space_strip_inner_cells_stay_inside_the_block() {
    let owners = owners_of(&[
        (0, 100, Kind::Code),
        (100, 400, Kind::Data),
        (500, 400, Kind::Bss),
        (900, 100, Kind::Code),
    ]);
    let region = region(Some(1000), &[(100, 800, Kind::Data)]);
    let found = inner(&owners, &block(100, 800), &true_scale(&region), 100);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].blocks, 1..2);
    assert_eq!(found[1].blocks, 2..3);
    assert_eq!((found[0].x0, found[1].x1), (10, 90));
}

#[test]
fn space_strip_inner_cells_clip_an_owner_that_runs_past_the_block() {
    let owners = owners_of(&[(0, 300, Kind::Code), (300, 900, Kind::Data)]);
    let region = region(Some(1000), &[(0, 600, Kind::Code)]);
    let found = inner(&owners, &block(0, 600), &true_scale(&region), 200);
    assert_eq!(found.iter().map(|cell| cell.bytes).sum::<u64>(), 600);
    assert_eq!(found[1].bytes, 300);
}

#[test]
fn space_strip_inner_needs_a_block_wider_than_64_px() {
    let owners = owners_of(&[(0, 500, Kind::Code), (500, 500, Kind::Data)]);
    let region = region(Some(1000), &[(0, 1000, Kind::Code)]);
    let scale = true_scale(&region);
    assert!(inner(&owners, &block(0, 1000), &scale, 64).is_empty());
    assert_eq!(inner(&owners, &block(0, 1000), &scale, 65).len(), 2);
}

#[test]
fn space_strip_inner_skips_a_block_with_one_owner() {
    let owners = owners_of(&[(0, 1000, Kind::Code)]);
    let region = region(Some(1000), &[(0, 1000, Kind::Code)]);
    assert!(inner(&owners, &block(0, 1000), &true_scale(&region), 800).is_empty());
    assert!(
        inner(
            &Owners::default(),
            &block(0, 1000),
            &true_scale(&region),
            800
        )
        .is_empty()
    );
}

#[test]
fn space_strip_inner_merges_sub_pixel_owners_with_dominant_kind() {
    let items: Vec<_> = (0..1000_u64)
        .map(|at| {
            (
                at * 10,
                10,
                if at % 4 == 0 {
                    Kind::Rodata
                } else {
                    Kind::Code
                },
            )
        })
        .collect();
    let owners = owners_of(&items);
    let region = region(Some(10_000), &[(0, 10_000, Kind::Code)]);
    let found = inner(&owners, &block(0, 10_000), &true_scale(&region), 100);
    assert_eq!(found.len(), 100);
    assert!(
        found
            .iter()
            .all(|cell| (9..=11).contains(&cell.blocks.len()) && cell.kind == Kind::Code)
    );
    assert_eq!(found.iter().map(|cell| cell.bytes).sum::<u64>(), 10_000);
    assert!(found.windows(2).all(|pair| pair[0].x1 <= pair[1].x0));
}

#[test]
fn space_strip_inner_keeps_one_pixel_for_tiny_owners() {
    let owners = owners_of(&[
        (0, 5000, Kind::Code),
        (5000, 1, Kind::Data),
        (5001, 4999, Kind::Code),
    ]);
    let region = region(Some(10_000), &[(0, 10_000, Kind::Code)]);
    let found = inner(&owners, &block(0, 10_000), &true_scale(&region), 400);
    assert_eq!(found.len(), 3);
    assert_eq!(found[1].kind, Kind::Data);
    assert!(found.iter().all(|cell| cell.x1 - cell.x0 >= 1));
}

#[test]
fn space_strip_inner_follows_the_zoom_view() {
    let owners = owners_of(&[(0, 500, Kind::Code), (500, 500, Kind::Data)]);
    let region = region(Some(1000), &[(0, 1000, Kind::Code)]);
    let scale = Scale::new(&region, ScaleMode::True, Some(250..750));
    let found = inner(&owners, &block(0, 1000), &scale, 100);
    assert_eq!(found.len(), 2);
    assert_eq!((found[0].x0, found[0].x1), (0, 50));
    assert_eq!(found[0].bytes, 250);
    assert_eq!((found[1].x0, found[1].x1), (50, 100));
}
