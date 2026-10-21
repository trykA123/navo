use std::sync::Arc;

use rusty_automotive::space::{Kind, Region, RegionSource};

use super::{Share, share_of, share_text};

fn flash() -> Vec<Arc<Region>> {
    vec![Arc::new(Region {
        name: "pflash".into(),
        start: Some(0x1000),
        start_estimated: false,
        total: Some(0x1000),
        chip: None,
        reserved: None,
        used: 0x400,
        by_kind: [0; Kind::COUNT],
        blocks: Vec::new(),
        source: RegionSource::Mdf,
    })]
}

#[test]
fn a_sized_symbol_shows_its_exact_share() {
    let share = share_of(&flash(), (0x1100, 0x100), Some(0x10)).expect("share");
    assert_eq!(
        share,
        Share {
            fraction: 0.25,
            estimated: false
        }
    );
    assert!(!share_text(share).contains('~'));
}

#[test]
fn a_zero_size_symbol_with_an_estimate_shows_a_tilde() {
    let share = share_of(&flash(), (0x1100, 0), Some(0x100)).expect("share");
    assert!(share.estimated);
    assert_eq!(share_text(share), "~25.0%");
}

#[test]
fn a_zero_size_symbol_without_an_estimate_stays_blank() {
    assert_eq!(share_of(&flash(), (0x1100, 0), None), None);
}
