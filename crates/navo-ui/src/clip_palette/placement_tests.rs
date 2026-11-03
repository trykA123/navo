use super::*;

const SCREEN: Rect = Rect {
    x: 0.,
    y: 0.,
    width: 1920.,
    height: 1040.,
};
const SMALL: Rect = Rect {
    x: 0.,
    y: 0.,
    width: 800.,
    height: 500.,
};

fn remembered(width: f32, height: f32) -> Option<Remembered> {
    Some(Remembered {
        display: None,
        rect: Rect {
            x: 10.,
            y: 10.,
            width,
            height,
        },
    })
}

#[test]
fn the_default_size_fits_a_large_display() {
    assert_eq!(open_size(None, SCREEN), DEFAULT_SIZE);
}

#[test]
fn the_default_size_is_clamped_to_ninety_percent_of_a_small_display() {
    let size = open_size(None, SMALL);
    assert_eq!((size.width, size.height), (720., 450.));
}

#[test]
fn a_remembered_size_is_used_and_clamped() {
    let size = open_size(remembered(1200., 700.), SCREEN);
    assert_eq!((size.width, size.height), (1200., 700.));
    let size = open_size(remembered(1200., 700.), SMALL);
    assert_eq!((size.width, size.height), (720., 450.));
}

#[test]
fn the_minimum_size_holds_unless_the_display_is_smaller() {
    let size = open_size(remembered(100., 100.), SCREEN);
    assert_eq!((size.width, size.height), (560., 320.));
    let tiny = Rect {
        width: 400.,
        height: 300.,
        ..SMALL
    };
    let size = open_size(remembered(100., 100.), tiny);
    assert_eq!((size.width, size.height), (360., 270.));
}

#[test]
fn center_puts_the_pop_up_in_the_middle() {
    let rect = place(
        PopUpPosition::Center,
        DEFAULT_SIZE,
        SCREEN,
        (Some((5., 5.)), Some((9., 9.))),
    );
    assert_eq!((rect.x, rect.y), (480., 220.));
}

#[test]
fn cursor_centres_on_the_mouse() {
    let rect = place(
        PopUpPosition::Cursor,
        DEFAULT_SIZE,
        SCREEN,
        (Some((960., 520.)), None),
    );
    assert_eq!((rect.x, rect.y), (480., 220.));
}

#[test]
fn cursor_near_an_edge_stays_fully_inside_the_display() {
    let rect = place(
        PopUpPosition::Cursor,
        DEFAULT_SIZE,
        SCREEN,
        (Some((1900., 1030.)), None),
    );
    assert_eq!((rect.x, rect.y), (960., 440.));
    let rect = place(
        PopUpPosition::Cursor,
        DEFAULT_SIZE,
        SCREEN,
        (Some((0., 0.)), None),
    );
    assert_eq!((rect.x, rect.y), (0., 0.));
}

#[test]
fn cursor_without_a_position_centres() {
    let rect = place(PopUpPosition::Cursor, DEFAULT_SIZE, SCREEN, (None, None));
    assert_eq!((rect.x, rect.y), (480., 220.));
}

#[test]
fn last_reopens_where_it_closed_and_centres_without_a_record() {
    let rect = place(
        PopUpPosition::Last,
        DEFAULT_SIZE,
        SCREEN,
        (None, Some((100., 50.))),
    );
    assert_eq!((rect.x, rect.y), (100., 50.));
    let rect = place(PopUpPosition::Last, DEFAULT_SIZE, SCREEN, (None, None));
    assert_eq!((rect.x, rect.y), (480., 220.));
}

#[test]
fn last_is_pulled_back_when_the_display_got_smaller() {
    let rect = place(
        PopUpPosition::Last,
        DEFAULT_SIZE,
        SCREEN,
        (None, Some((1800., 900.))),
    );
    assert_eq!((rect.x, rect.y), (960., 440.));
}

#[test]
fn rects_within_a_pixel_count_as_the_same() {
    let a = place(PopUpPosition::Center, DEFAULT_SIZE, SCREEN, (None, None));
    let b = Rect { x: a.x + 0.5, ..a };
    assert!(a.same_as(b));
    assert!(!a.same_as(Rect { x: a.x + 2., ..a }));
}
