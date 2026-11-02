use super::*;

#[test]
fn opposite_is_an_involution() {
    for d in Direction::ALL {
        assert_eq!(d.opposite().opposite(), d);
        assert_ne!(d.opposite(), d);
    }
}

#[test]
fn keywords_round_trip() {
    for d in Direction::ALL {
        assert_eq!(Direction::from_keyword(d.keyword()), Some(d));
    }
    assert_eq!(Direction::from_keyword("Left"), None);
}

#[test]
fn horizontal_is_left_and_right() {
    assert!(Direction::Left.is_horizontal() && Direction::Right.is_horizontal());
    assert!(!Direction::Up.is_horizontal() && !Direction::Down.is_horizontal());
}
