use rusty_automotive::space::{Rect, Target, Tile};

use super::{caption, dominant, inner_name, pick_of};

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
fn the_biggest_kind_sets_the_colour() {
    assert_eq!(dominant(&[10, 400, 20, 0, 0, 0]), 1);
    assert_eq!(dominant(&[0; 6]), 0);
}
