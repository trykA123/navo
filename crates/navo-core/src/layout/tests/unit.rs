use super::{assert_rects, p, rect};
use crate::{Direction, Layout, LayoutError};

fn side_by_side() -> Layout {
    let mut layout = Layout::new(p(1));
    layout.split(Direction::Right, p(2)).unwrap();
    layout
}

fn l_shape() -> Layout {
    let mut layout = side_by_side();
    layout.split(Direction::Down, p(3)).unwrap();
    layout
}

#[test]
fn new_layout_has_one_focused_pane() {
    let layout = Layout::new(p(1));
    assert_eq!(layout.panes(), vec![p(1)]);
    assert_eq!(layout.focused(), p(1));
}

#[test]
fn new_layout_covers_the_unit_square() {
    assert_rects(&Layout::new(p(1)), &[(1, rect(0.0, 0.0, 1.0, 1.0))]);
}

#[test]
fn contains_only_the_panes_of_the_layout() {
    let layout = side_by_side();
    assert!(layout.contains(p(1)) && layout.contains(p(2)));
    assert!(!layout.contains(p(3)));
}

#[test]
fn split_right_puts_the_new_pane_in_the_right_half() {
    assert_rects(
        &side_by_side(),
        &[(1, rect(0.0, 0.0, 0.5, 1.0)), (2, rect(0.5, 0.0, 0.5, 1.0))],
    );
}

#[test]
fn split_left_puts_the_new_pane_in_the_left_half() {
    let mut layout = Layout::new(p(1));
    layout.split(Direction::Left, p(2)).unwrap();
    assert_rects(
        &layout,
        &[(2, rect(0.0, 0.0, 0.5, 1.0)), (1, rect(0.5, 0.0, 0.5, 1.0))],
    );
}

#[test]
fn split_down_puts_the_new_pane_in_the_bottom_half() {
    let mut layout = Layout::new(p(1));
    layout.split(Direction::Down, p(2)).unwrap();
    assert_rects(
        &layout,
        &[(1, rect(0.0, 0.0, 1.0, 0.5)), (2, rect(0.0, 0.5, 1.0, 0.5))],
    );
}

#[test]
fn split_up_puts_the_new_pane_in_the_top_half() {
    let mut layout = Layout::new(p(1));
    layout.split(Direction::Up, p(2)).unwrap();
    assert_rects(
        &layout,
        &[(2, rect(0.0, 0.0, 1.0, 0.5)), (1, rect(0.0, 0.5, 1.0, 0.5))],
    );
}

#[test]
fn split_focuses_the_new_pane() {
    for direction in Direction::ALL {
        let mut layout = Layout::new(p(1));
        layout.split(direction, p(2)).unwrap();
        assert_eq!(layout.focused(), p(2), "{direction:?}");
    }
}

#[test]
fn split_divides_only_the_focused_pane() {
    assert_rects(
        &l_shape(),
        &[
            (1, rect(0.0, 0.0, 0.5, 1.0)),
            (2, rect(0.5, 0.0, 0.5, 0.5)),
            (3, rect(0.5, 0.5, 0.5, 0.5)),
        ],
    );
}

#[test]
fn split_with_a_pane_already_present_is_rejected() {
    let mut layout = side_by_side();
    let before = layout.clone();
    assert_eq!(
        layout.split(Direction::Down, p(1)),
        Err(LayoutError::DuplicatePane(p(1)))
    );
    assert_eq!(layout, before);
}

#[test]
fn close_gives_the_parent_rect_to_the_sibling() {
    let mut layout = l_shape();
    layout.close(p(3)).unwrap();
    assert_rects(
        &layout,
        &[(1, rect(0.0, 0.0, 0.5, 1.0)), (2, rect(0.5, 0.0, 0.5, 1.0))],
    );
}

#[test]
fn close_moves_focus_to_the_sibling_leaf() {
    let mut layout = l_shape();
    layout.close(p(3)).unwrap();
    assert_eq!(layout.focused(), p(2));
}

#[test]
fn close_moves_focus_to_the_first_leaf_of_the_sibling_subtree() {
    let mut layout = l_shape();
    layout.focus(p(1)).unwrap();
    layout.close(p(1)).unwrap();
    assert_eq!(layout.focused(), p(2));
}

#[test]
fn close_of_a_subtree_sibling_keeps_the_subtree_shape() {
    let mut layout = l_shape();
    layout.close(p(1)).unwrap();
    assert_rects(
        &layout,
        &[(2, rect(0.0, 0.0, 1.0, 0.5)), (3, rect(0.0, 0.5, 1.0, 0.5))],
    );
}

#[test]
fn close_of_an_unfocused_pane_keeps_the_focus() {
    let mut layout = l_shape();
    layout.close(p(1)).unwrap();
    assert_eq!(layout.focused(), p(3));
}

#[test]
fn close_of_the_last_pane_is_rejected() {
    let mut layout = Layout::new(p(1));
    assert_eq!(layout.close(p(1)), Err(LayoutError::LastPane));
    assert_eq!(layout, Layout::new(p(1)));
}

#[test]
fn close_of_an_unknown_pane_is_rejected() {
    let mut layout = side_by_side();
    let before = layout.clone();
    assert_eq!(layout.close(p(9)), Err(LayoutError::UnknownPane(p(9))));
    assert_eq!(layout, before);
}

#[test]
fn focus_moves_to_a_known_pane() {
    let mut layout = side_by_side();
    layout.focus(p(1)).unwrap();
    assert_eq!(layout.focused(), p(1));
}

#[test]
fn focus_of_an_unknown_pane_is_rejected() {
    let mut layout = side_by_side();
    let before = layout.clone();
    assert_eq!(layout.focus(p(9)), Err(LayoutError::UnknownPane(p(9))));
    assert_eq!(layout, before);
}

#[test]
fn neighbour_tie_breaks_to_the_top_most_pane() {
    assert_eq!(l_shape().neighbour(p(1), Direction::Right), Some(p(2)));
}

#[test]
fn neighbour_tie_breaks_to_the_left_most_pane() {
    let mut layout = Layout::new(p(1));
    layout.split(Direction::Down, p(2)).unwrap();
    layout.split(Direction::Right, p(3)).unwrap();
    assert_eq!(layout.neighbour(p(1), Direction::Down), Some(p(2)));
}

#[test]
fn neighbour_prefers_the_largest_overlap() {
    let mut layout = side_by_side();
    layout.split(Direction::Up, p(3)).unwrap();
    layout.split(Direction::Up, p(4)).unwrap();
    assert_eq!(layout.neighbour(p(1), Direction::Right), Some(p(2)));
}

#[test]
fn neighbour_finds_the_full_height_pane_from_either_stacked_pane() {
    let layout = l_shape();
    assert_eq!(layout.neighbour(p(2), Direction::Left), Some(p(1)));
    assert_eq!(layout.neighbour(p(3), Direction::Left), Some(p(1)));
}

#[test]
fn neighbour_follows_a_stacked_split() {
    let layout = l_shape();
    assert_eq!(layout.neighbour(p(2), Direction::Down), Some(p(3)));
    assert_eq!(layout.neighbour(p(3), Direction::Up), Some(p(2)));
}

#[test]
fn neighbour_is_none_at_the_edge() {
    let layout = l_shape();
    assert_eq!(layout.neighbour(p(1), Direction::Left), None);
    assert_eq!(layout.neighbour(p(1), Direction::Up), None);
    assert_eq!(layout.neighbour(p(2), Direction::Right), None);
}

#[test]
fn neighbour_of_an_unknown_pane_is_none() {
    assert_eq!(l_shape().neighbour(p(9), Direction::Left), None);
}

#[test]
fn focus_direction_moves_focus_to_the_neighbour() {
    let mut layout = side_by_side();
    assert_eq!(layout.focus_direction(Direction::Left), Ok(p(1)));
    assert_eq!(layout.focused(), p(1));
}

#[test]
fn focus_direction_without_a_neighbour_is_rejected() {
    let mut layout = side_by_side();
    let before = layout.clone();
    assert_eq!(
        layout.focus_direction(Direction::Right),
        Err(LayoutError::NoNeighbour(Direction::Right))
    );
    assert_eq!(layout, before);
}

#[test]
fn swap_exchanges_the_places_of_two_panes() {
    let mut layout = side_by_side();
    layout.swap(Direction::Left).unwrap();
    assert_eq!(layout.panes(), vec![p(2), p(1)]);
}

#[test]
fn swap_keeps_the_focus_on_the_moved_pane() {
    let mut layout = side_by_side();
    layout.swap(Direction::Left).unwrap();
    assert_eq!(layout.focused(), p(2));
}

#[test]
fn swap_across_subtrees_exchanges_the_rects() {
    let mut layout = l_shape();
    layout.swap(Direction::Left).unwrap();
    assert_rects(
        &layout,
        &[
            (3, rect(0.0, 0.0, 0.5, 1.0)),
            (2, rect(0.5, 0.0, 0.5, 0.5)),
            (1, rect(0.5, 0.5, 0.5, 0.5)),
        ],
    );
}

#[test]
fn swap_without_a_neighbour_is_rejected() {
    let mut layout = side_by_side();
    let before = layout.clone();
    assert_eq!(
        layout.swap(Direction::Right),
        Err(LayoutError::NoNeighbour(Direction::Right))
    );
    assert_eq!(layout, before);
}

#[test]
fn resize_right_grows_the_first_pane() {
    let mut layout = side_by_side();
    layout.focus(p(1)).unwrap();
    layout.resize(Direction::Right, 100).unwrap();
    assert_rects(
        &layout,
        &[(1, rect(0.0, 0.0, 0.6, 1.0)), (2, rect(0.6, 0.0, 0.4, 1.0))],
    );
}

#[test]
fn resize_left_grows_the_second_pane_and_shrinks_the_first() {
    let mut layout = side_by_side();
    layout.resize(Direction::Left, 100).unwrap();
    assert_rects(
        &layout,
        &[(1, rect(0.0, 0.0, 0.4, 1.0)), (2, rect(0.4, 0.0, 0.6, 1.0))],
    );
}

#[test]
fn resize_down_grows_the_top_pane() {
    let mut layout = l_shape();
    layout.focus(p(2)).unwrap();
    layout.resize(Direction::Down, 250).unwrap();
    assert_rects(
        &layout,
        &[
            (1, rect(0.0, 0.0, 0.5, 1.0)),
            (2, rect(0.5, 0.0, 0.5, 0.75)),
            (3, rect(0.5, 0.75, 0.5, 0.25)),
        ],
    );
}

#[test]
fn resize_up_grows_the_bottom_pane() {
    let mut layout = l_shape();
    layout.resize(Direction::Up, 250).unwrap();
    assert_rects(
        &layout,
        &[
            (1, rect(0.0, 0.0, 0.5, 1.0)),
            (2, rect(0.5, 0.0, 0.5, 0.25)),
            (3, rect(0.5, 0.25, 0.5, 0.75)),
        ],
    );
}

#[test]
fn resize_clamps_the_ratio_at_900() {
    let mut layout = side_by_side();
    layout.focus(p(1)).unwrap();
    layout.resize(Direction::Right, 1000).unwrap();
    assert_rects(
        &layout,
        &[(1, rect(0.0, 0.0, 0.9, 1.0)), (2, rect(0.9, 0.0, 0.1, 1.0))],
    );
}

#[test]
fn resize_clamps_the_ratio_at_100() {
    let mut layout = side_by_side();
    layout.resize(Direction::Left, 1000).unwrap();
    assert_rects(
        &layout,
        &[(1, rect(0.0, 0.0, 0.1, 1.0)), (2, rect(0.1, 0.0, 0.9, 1.0))],
    );
}

#[test]
fn resize_by_the_largest_delta_clamps_without_overflow() {
    let mut layout = side_by_side();
    layout.focus(p(1)).unwrap();
    layout.resize(Direction::Right, u16::MAX).unwrap();
    assert_rects(
        &layout,
        &[(1, rect(0.0, 0.0, 0.9, 1.0)), (2, rect(0.9, 0.0, 0.1, 1.0))],
    );
}

#[test]
fn resize_of_a_single_pane_is_rejected() {
    let mut layout = Layout::new(p(1));
    assert_eq!(
        layout.resize(Direction::Right, 100),
        Err(LayoutError::NoSplitToResize(Direction::Right))
    );
    assert_eq!(layout, Layout::new(p(1)));
}

#[test]
fn resize_across_the_wrong_axis_is_rejected() {
    let mut layout = side_by_side();
    let before = layout.clone();
    assert_eq!(
        layout.resize(Direction::Down, 100),
        Err(LayoutError::NoSplitToResize(Direction::Down))
    );
    assert_eq!(layout, before);
}

#[test]
fn resize_towards_the_outer_edge_moves_the_far_border() {
    let mut layout = side_by_side();
    layout.resize(Direction::Right, 100).unwrap();
    assert_rects(
        &layout,
        &[(1, rect(0.0, 0.0, 0.6, 1.0)), (2, rect(0.6, 0.0, 0.4, 1.0))],
    );
}

#[test]
fn resize_changes_the_nearest_matching_split() {
    let mut layout = side_by_side();
    layout.split(Direction::Right, p(3)).unwrap();
    layout.focus(p(2)).unwrap();
    layout.resize(Direction::Right, 100).unwrap();
    assert_rects(
        &layout,
        &[
            (1, rect(0.0, 0.0, 0.5, 1.0)),
            (2, rect(0.5, 0.0, 0.3, 1.0)),
            (3, rect(0.8, 0.0, 0.2, 1.0)),
        ],
    );
}

#[test]
fn resize_skips_a_split_where_the_pane_is_on_the_wrong_side() {
    let mut layout = side_by_side();
    layout.split(Direction::Right, p(3)).unwrap();
    layout.focus(p(2)).unwrap();
    layout.resize(Direction::Left, 100).unwrap();
    assert_rects(
        &layout,
        &[
            (1, rect(0.0, 0.0, 0.4, 1.0)),
            (2, rect(0.4, 0.0, 0.3, 1.0)),
            (3, rect(0.7, 0.0, 0.3, 1.0)),
        ],
    );
}

#[test]
fn resize_skips_splits_of_the_other_axis() {
    let mut layout = l_shape();
    layout.focus(p(2)).unwrap();
    layout.resize(Direction::Left, 100).unwrap();
    assert_rects(
        &layout,
        &[
            (1, rect(0.0, 0.0, 0.4, 1.0)),
            (2, rect(0.4, 0.0, 0.6, 0.5)),
            (3, rect(0.4, 0.5, 0.6, 0.5)),
        ],
    );
}

#[test]
fn numbering_follows_reading_order_for_all_builtin_shapes() {
    let side = side_by_side();
    let mut columns = side_by_side();
    columns.split(Direction::Right, p(3)).unwrap();
    columns
        .set_ratio(&crate::layout::SplitPath(vec![]), 330)
        .unwrap();
    let mut wide = Layout::new(p(1));
    wide.split(Direction::Down, p(2)).unwrap();
    wide.split(Direction::Right, p(3)).unwrap();
    wide.set_ratio(&crate::layout::SplitPath(vec![]), 520)
        .unwrap();
    let mut grid = l_shape();
    grid.focus(p(1)).unwrap();
    grid.split(Direction::Down, p(4)).unwrap();
    let mut main = l_shape();
    main.split(Direction::Down, p(4)).unwrap();
    main.set_ratio(&crate::layout::SplitPath(vec![]), 600)
        .unwrap();
    main.set_ratio(&crate::layout::SplitPath(vec![true]), 330)
        .unwrap();
    for (layout, expected) in [
        (side, vec![p(1), p(2)]),
        (columns, vec![p(1), p(2), p(3)]),
        (wide, vec![p(1), p(2), p(3)]),
        (grid, vec![p(1), p(2), p(4), p(3)]),
        (main, vec![p(1), p(2), p(3), p(4)]),
    ] {
        assert_eq!(layout.numbered(), expected);
    }
}

#[test]
fn numbering_treats_nearly_aligned_rows_as_equal() {
    let mut layout = Layout::new(p(5));
    layout.split(Direction::Down, p(1)).unwrap();
    layout.split(Direction::Right, p(2)).unwrap();
    layout.split(Direction::Down, p(3)).unwrap();
    layout.focus(p(1)).unwrap();
    layout.split(Direction::Down, p(4)).unwrap();
    layout
        .set_ratio(&crate::layout::SplitPath(vec![true, true]), 501)
        .unwrap();
    assert_eq!(layout.numbered(), vec![p(5), p(1), p(2), p(4), p(3)]);
}

#[test]
fn swap_panes_exchanges_any_two_leaves_and_keeps_focus() {
    let mut layout = l_shape();
    layout.swap_panes(p(1), p(3)).unwrap();
    assert_eq!(layout.numbered(), vec![p(3), p(2), p(1)]);
    assert_eq!(layout.focused(), p(3));
    let before = layout.clone();
    layout.swap_panes(p(3), p(3)).unwrap();
    assert_eq!(layout, before);
}

#[test]
fn swap_panes_rejects_unknown_ids_without_mutating() {
    let mut layout = l_shape();
    let before = layout.clone();
    for (a, b) in [(p(9), p(1)), (p(1), p(9)), (p(9), p(9))] {
        assert_eq!(layout.swap_panes(a, b), Err(LayoutError::UnknownPane(p(9))));
        assert_eq!(layout, before);
    }
}

#[test]
fn set_ratio_rejects_leaf_and_missing_paths_without_mutating() {
    use crate::layout::SplitPath;
    let mut layout = l_shape();
    let before = layout.clone();
    for path in [SplitPath(vec![false]), SplitPath(vec![false, true])] {
        assert_eq!(
            layout.set_ratio(&path, 700),
            Err(LayoutError::InvalidSplitPath(path))
        );
        assert_eq!(layout, before);
    }
    let mut single = Layout::new(p(1));
    assert!(single.set_ratio(&SplitPath(vec![]), 700).is_err());
}

#[test]
fn splits_expose_paths_axes_ratios_and_containing_rectangles() {
    use crate::layout::{Axis, SplitInfo, SplitPath};
    assert!(Layout::new(p(1)).splits().is_empty());
    assert_eq!(
        l_shape().splits(),
        vec![
            SplitInfo {
                path: SplitPath(vec![]),
                axis: Axis::SideBySide,
                ratio: 500,
                rect: rect(0.0, 0.0, 1.0, 1.0)
            },
            SplitInfo {
                path: SplitPath(vec![true]),
                axis: Axis::Stacked,
                ratio: 500,
                rect: rect(0.5, 0.0, 0.5, 1.0)
            },
        ]
    );
}

#[test]
fn set_ratio_clamps_both_bounds_and_changes_only_the_selected_split() {
    use crate::layout::SplitPath;
    let mut layout = l_shape();
    for (requested, expected) in [(0, 100), (u16::MAX, 900), (700, 700)] {
        layout.set_ratio(&SplitPath(vec![true]), requested).unwrap();
        let splits = layout.splits();
        assert_eq!(splits[0].ratio, 500);
        assert_eq!(splits[1].ratio, expected);
    }
}

#[test]
fn even_weights_splits_by_leaf_count() {
    use crate::layout::SplitPath;
    let mut layout = side_by_side();
    layout.split(Direction::Right, p(3)).unwrap();
    layout.set_ratio(&SplitPath(vec![]), 700).unwrap();
    let focused = layout.focused();
    layout.even();
    assert_eq!(
        layout.splits().iter().map(|s| s.ratio).collect::<Vec<_>>(),
        vec![333, 500]
    );
    assert_eq!(layout.focused(), focused);
    let before = layout.clone();
    layout.even();
    assert_eq!(layout, before);
    let mut single = Layout::new(p(1));
    single.even();
    assert_eq!(single, Layout::new(p(1)));
}
