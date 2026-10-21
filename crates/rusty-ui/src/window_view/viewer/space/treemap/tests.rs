use rusty_automotive::space::{Item, Part, Rect, Target, Tile, layout};

use super::palette::palette;
use super::{caption, inner_name, pick_of, shade_of};
use crate::tokens::{Mode, Theme};

fn tile(target: Target, text: bool) -> Tile {
    Tile {
        target,
        value: 2048,
        rect: Rect::new(0., 0., 100., 50.),
        text,
        inner: Vec::new(),
    }
}

#[test]
fn text_follows_the_layout_flag() {
    let name = "Com";
    assert!(caption(&tile(Target::Item(1), false), name, 4096).is_none());
    let (head, detail) = caption(&tile(Target::Item(1), true), name, 4096).expect("text");
    assert_eq!(head, "Com");
    assert!(detail.contains("2.0 KB"));
    assert!(detail.contains("50.0"));
}

#[test]
fn an_inner_tile_named_like_its_parent_draws_no_second_label() {
    let parent = "(not attributed)";
    assert_eq!(inner_name(parent.to_owned(), parent), None);
    assert_eq!(inner_name("Com".to_owned(), parent), Some("Com".to_owned()));
}

#[test]
fn the_more_tile_picks_nothing() {
    assert_eq!(pick_of(&tile(Target::More { count: 3 }, true)), None);
    assert_eq!(pick_of(&tile(Target::Item(7), true)), Some(7));
}

#[test]
fn inner_tiles_take_shades_of_their_group_and_leaves_take_its_base() {
    let theme = Theme::new(Mode::Light);
    let colours = palette(4, Mode::Light);
    let group = colours[2];
    let part = tile(Target::Item(3), true);
    assert_eq!(
        shade_of(&theme, &part, (Some(&group), Some(1))),
        group.part(1)
    );
    assert_eq!(shade_of(&theme, &part, (Some(&group), None)), group.base());
    assert_ne!(
        shade_of(&theme, &part, (Some(&colours[0]), Some(1))).bg,
        shade_of(&theme, &part, (Some(&group), Some(1))).bg
    );
}

#[test]
fn the_more_tile_keeps_the_neutral_style_in_any_group() {
    let theme = Theme::new(Mode::Dark);
    let group = palette(2, Mode::Dark)[1];
    let more = tile(Target::More { count: 4 }, true);
    let shade = shade_of(&theme, &more, (Some(&group), Some(0)));
    assert_eq!(shade.bg, theme.mem.free);
}

#[test]
fn inner_captions_appear_only_on_tiles_above_the_text_threshold() {
    let parts = (0..12)
        .map(|id| Part {
            id,
            value: 4096 >> (id / 2),
        })
        .collect();
    let item = Item {
        id: 0,
        value: 40_000,
        parts,
    };
    let tiles = layout(&[item], Rect::new(0., 0., 800., 500.));
    let inner = &tiles[0].inner;
    let total: u64 = inner.iter().map(|tile| tile.value).sum();
    let captions: Vec<bool> = inner
        .iter()
        .map(|tile| caption(tile, "Part", total).is_some())
        .collect();
    assert!(captions[0]);
    assert!(captions.iter().any(|shown| !shown));
    assert!(
        inner
            .iter()
            .zip(&captions)
            .all(|(tile, shown)| tile.text == *shown)
    );
}
