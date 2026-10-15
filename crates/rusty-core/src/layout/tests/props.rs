use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use super::{EPS, p};
use crate::{Direction, Layout, LayoutError, PaneId, Rect};

const UNKNOWN: u64 = u64::MAX;

#[derive(Debug, Clone)]
enum Op {
    SplitFresh(Direction),
    SplitExisting(Direction, usize),
    Close(Option<usize>),
    Focus(Option<usize>),
    FocusDirection(Direction),
    Swap(Direction),
    Resize(Direction, u16),
}

fn direction() -> impl Strategy<Value = Direction> {
    prop::sample::select(Direction::ALL.to_vec())
}

fn pick() -> impl Strategy<Value = Option<usize>> {
    prop::option::weighted(0.8, any::<usize>())
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => direction().prop_map(Op::SplitFresh),
        1 => (direction(), any::<usize>()).prop_map(|(d, i)| Op::SplitExisting(d, i)),
        2 => pick().prop_map(Op::Close),
        1 => pick().prop_map(Op::Focus),
        2 => direction().prop_map(Op::FocusDirection),
        1 => direction().prop_map(Op::Swap),
        2 => (direction(), 0..=1000_u16).prop_map(|(d, delta)| Op::Resize(d, delta)),
    ]
}

fn target(layout: &Layout, pick: Option<usize>) -> PaneId {
    let panes = layout.panes();
    pick.map_or(p(UNKNOWN), |i| panes[i % panes.len()])
}

fn apply(layout: &mut Layout, op: &Op, next_id: &mut u64) -> Result<(), LayoutError> {
    match *op {
        Op::SplitFresh(d) => {
            *next_id += 1;
            layout.split(d, p(*next_id))
        }
        Op::SplitExisting(d, i) => {
            let pane = target(layout, Some(i));
            layout.split(d, pane)
        }
        Op::Close(pick) => layout.close(target(layout, pick)),
        Op::Focus(pick) => layout.focus(target(layout, pick)),
        Op::FocusDirection(d) => layout.focus_direction(d).map(|_| ()),
        Op::Swap(d) => layout.swap(d),
        Op::Resize(d, delta) => layout.resize(d, delta),
    }
}

fn check_panes(layout: &Layout) -> Result<(), TestCaseError> {
    let panes = layout.panes();
    prop_assert!(!panes.is_empty());
    let mut sorted = panes.clone();
    sorted.sort();
    sorted.dedup();
    prop_assert_eq!(sorted.len(), panes.len(), "duplicate panes");
    prop_assert!(panes.contains(&layout.focused()));
    for &pane in &panes {
        prop_assert!(layout.contains(pane));
    }
    prop_assert!(!layout.contains(p(UNKNOWN)));
    Ok(())
}

fn overlap_area(a: Rect, b: Rect) -> f64 {
    let w = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let h = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    w.max(0.0) * h.max(0.0)
}

fn check_rects(layout: &Layout) -> Result<(), TestCaseError> {
    let rects = layout.rects();
    let ids: Vec<PaneId> = rects.iter().map(|(id, _)| *id).collect();
    prop_assert_eq!(ids, layout.panes());
    for (_, r) in &rects {
        prop_assert!(r.width > 0.0 && r.height > 0.0, "{:?}", r);
        prop_assert!(r.x >= -EPS && r.y >= -EPS, "{:?}", r);
        prop_assert!(r.x + r.width <= 1.0 + EPS, "{:?}", r);
        prop_assert!(r.y + r.height <= 1.0 + EPS, "{:?}", r);
    }
    let area: f64 = rects.iter().map(|(_, r)| r.width * r.height).sum();
    prop_assert!((area - 1.0).abs() < EPS, "total area {}", area);
    for (i, (_, a)) in rects.iter().enumerate() {
        for (_, b) in &rects[i + 1..] {
            prop_assert!(overlap_area(*a, *b) <= EPS, "{:?} overlaps {:?}", a, b);
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn random_operations_keep_the_layout_valid(ops in prop::collection::vec(op(), 0..=40)) {
        let mut layout = Layout::new(p(0));
        let mut next_id = 0;
        check_panes(&layout)?;
        check_rects(&layout)?;
        for op in &ops {
            let before = layout.clone();
            if apply(&mut layout, op, &mut next_id).is_err() {
                prop_assert_eq!(&layout, &before, "failed {:?} changed the layout", op);
            }
            check_panes(&layout)?;
            check_rects(&layout)?;
        }
    }

    #[test]
    fn neighbour_is_symmetric_side_by_side(
        split in prop::sample::select(vec![Direction::Left, Direction::Right]),
        grow in prop::sample::select(vec![Direction::Left, Direction::Right]),
        delta in 0..=1000_u16,
    ) {
        let mut layout = Layout::new(p(1));
        layout.split(split, p(2))?;
        let _ = layout.resize(grow, delta);
        for a in layout.panes() {
            if let Some(b) = layout.neighbour(a, Direction::Right) {
                prop_assert_eq!(layout.neighbour(b, Direction::Left), Some(a));
            }
        }
    }
}
