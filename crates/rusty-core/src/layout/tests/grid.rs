use super::{p, rect};
use crate::session::LayoutState;
use crate::{Direction, Layout, Tiles};

fn leaf(id: u64) -> Box<LayoutState> {
    Box::new(LayoutState::Pane(id))
}

fn split(
    side_by_side: bool,
    ratio: u16,
    first: Box<LayoutState>,
    second: Box<LayoutState>,
) -> LayoutState {
    LayoutState::Split {
        side_by_side,
        ratio,
        first,
        second,
    }
}

fn grid() -> Layout {
    let mut layout = Layout::new(p(1));
    layout.apply(&Tiles::builtin()[3].tiles, &mut {
        let mut next = 1;
        move || {
            next += 1;
            p(next)
        }
    });
    layout
}

fn widths(layout: &Layout) -> Vec<f64> {
    layout
        .rects()
        .iter()
        .map(|(_, rect)| (rect.width * 1000.0).round() / 1000.0)
        .collect()
}

#[test]
fn an_old_shared_divider_loads_into_both_rows() {
    let old = split(
        true,
        300,
        Box::new(split(false, 600, leaf(1), leaf(2))),
        Box::new(split(false, 400, leaf(3), leaf(4))),
    );
    let layout = Layout::from_state(&old, p(1)).unwrap();
    let rects: Vec<_> = layout.rects();
    let find = |id| rects.iter().find(|(pane, _)| *pane == p(id)).unwrap().1;
    assert_eq!(find(1), rect(0.0, 0.0, 0.3, 0.6));
    assert_eq!(find(3), rect(0.3, 0.0, 0.7, 0.6));
    assert_eq!(find(2), rect(0.0, 0.6, 0.3, 0.4));
    assert_eq!(find(4), rect(0.3, 0.6, 0.7, 0.4));
    assert_eq!(layout.splits().len(), 3);
}

#[test]
fn a_saved_grid_in_the_new_shape_loads_unchanged() {
    let layout = grid();
    let again = Layout::from_state(&layout.to_state(), p(1)).unwrap();
    assert_eq!(again, layout);
}

#[test]
fn dragging_the_top_divider_leaves_the_bottom_row_alone() {
    let mut layout = grid();
    layout.set_ratio(&"1".parse().unwrap(), 700).unwrap();
    assert_eq!(widths(&layout), vec![0.7, 0.3, 0.5, 0.5]);
}

#[test]
fn resizing_horizontally_moves_only_the_focused_row() {
    let mut layout = grid();
    layout.focus(p(1)).unwrap();
    layout.resize(Direction::Right, 100).unwrap();
    assert_eq!(widths(&layout), vec![0.6, 0.4, 0.5, 0.5]);
    layout.focus(p(4)).unwrap();
    layout.resize(Direction::Left, 100).unwrap();
    assert_eq!(widths(&layout), vec![0.6, 0.4, 0.4, 0.6]);
}

#[test]
fn even_resets_every_divider() {
    let mut layout = grid();
    layout.resize(Direction::Right, 200).unwrap();
    layout.focus(p(4)).unwrap();
    layout.resize(Direction::Down, 100).unwrap();
    layout.even();
    assert_eq!(layout.to_tiles(), grid().to_tiles());
}
