use crate::{Layout, PaneId, Rect};

mod props;
mod unit;

mod apply {
    use proptest::prelude::*;

    use super::{Layout, p};
    use crate::Direction;
    use crate::layout::Tiles;

    fn apply(layout: &mut Layout, tiles: &Tiles, next: &mut u64) -> crate::layout::Applied {
        layout.apply(tiles, &mut || {
            *next += 1;
            p(*next)
        })
    }

    #[test]
    fn growing_keeps_old_numbers_and_focus() {
        let mut layout = Layout::new(p(0));
        layout.split(Direction::Left, p(1)).unwrap();
        let result = apply(&mut layout, &Tiles::builtin()[3].tiles, &mut 1);
        assert_eq!(result.added, vec![p(2), p(3)]);
        assert!(result.removed.is_empty());
        assert_eq!(layout.numbered(), vec![p(1), p(0), p(2), p(3)]);
        assert_eq!(layout.focused(), p(1));
        assert_eq!(layout.to_tiles(), Tiles::builtin()[3].tiles);
    }

    #[test]
    fn shrinking_removes_highest_numbers_first_and_restores_focus() {
        let mut layout = Layout::new(p(0));
        apply(&mut layout, &Tiles::builtin()[3].tiles, &mut 0);
        layout.focus(p(3)).unwrap();
        let result = layout.apply(&Tiles::builtin()[0].tiles, &mut || unreachable!());
        assert_eq!(result.removed, vec![p(3), p(2)]);
        assert!(result.added.is_empty());
        assert_eq!(layout.numbered(), vec![p(0), p(1)]);
        assert_eq!(layout.focused(), p(0));
    }

    #[test]
    fn shrinking_preserves_surviving_focus() {
        let mut layout = Layout::new(p(0));
        apply(&mut layout, &Tiles::builtin()[3].tiles, &mut 0);
        layout.focus(p(1)).unwrap();
        layout.apply(&Tiles::builtin()[0].tiles, &mut || unreachable!());
        assert_eq!(layout.focused(), p(1));
    }

    #[test]
    fn applying_current_shape_changes_nothing() {
        let mut layout = Layout::new(p(10));
        apply(&mut layout, &Tiles::builtin()[3].tiles, &mut 10);
        layout.swap_panes(p(10), p(13)).unwrap();
        layout.focus(p(12)).unwrap();
        let before = layout.clone();
        let result = layout.apply(&layout.to_tiles(), &mut || unreachable!());
        assert_eq!(layout, before);
        assert!(result.added.is_empty());
        assert!(result.removed.is_empty());
    }

    #[test]
    fn every_builtin_round_trips_through_tiles() {
        for preset in Tiles::builtin() {
            let mut layout = Layout::new(p(0));
            apply(&mut layout, &preset.tiles, &mut 0);
            assert_eq!(layout.to_tiles(), preset.tiles);
            let before = layout.clone();
            let tiles = layout.to_tiles();
            let result = layout.apply(&tiles, &mut || unreachable!());
            assert_eq!(layout, before);
            assert!(result.added.is_empty());
            assert!(result.removed.is_empty());
        }
    }

    #[test]
    fn applying_single_pane_keeps_number_one() {
        let mut layout = Layout::new(p(0));
        apply(&mut layout, &Tiles::builtin()[4].tiles, &mut 0);
        layout.focus(p(3)).unwrap();
        let result = layout.apply(&Tiles::Pane, &mut || unreachable!());
        assert_eq!(result.removed, vec![p(3), p(2), p(1)]);
        assert_eq!(layout, Layout::new(p(0)));
        assert_eq!(layout.to_tiles(), Tiles::Pane);
    }

    proptest! {
        #[test]
        fn repeated_presets_keep_ids_unique(
            choices in prop::collection::vec((0..6_usize, any::<usize>()), 0..40),
        ) {
            let presets = Tiles::builtin();
            let mut layout = Layout::new(p(0));
            let mut next = 0;
            for (choice, focus) in choices {
                let tiles = presets.get(choice).map_or(&Tiles::Pane, |preset| &preset.tiles);
                let old = layout.numbered();
                layout.focus(old[focus % old.len()]).unwrap();
                let focused = layout.focused();
                let result = apply(&mut layout, tiles, &mut next);
                let panes = layout.numbered();
                let mut unique = panes.clone();
                unique.sort_unstable();
                unique.dedup();
                prop_assert_eq!(unique.len(), panes.len());
                let kept = old.len().min(panes.len());
                prop_assert_eq!(&panes[..kept], &old[..kept]);
                prop_assert_eq!(&panes[kept..], &result.added);
                prop_assert_eq!(result.removed, old[kept..].iter().rev().copied().collect::<Vec<_>>());
                prop_assert_eq!(layout.focused(), if panes.contains(&focused) { focused } else { panes[0] });
                prop_assert_eq!(&layout.to_tiles(), tiles);
            }
        }
    }
}

const EPS: f64 = 1e-9;

fn p(raw: u64) -> PaneId {
    PaneId::new(raw)
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn close_to(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() < EPS
        && (a.y - b.y).abs() < EPS
        && (a.width - b.width).abs() < EPS
        && (a.height - b.height).abs() < EPS
}

#[track_caller]
fn assert_rects(layout: &Layout, expected: &[(u64, Rect)]) {
    let actual = layout.rects();
    let ids: Vec<PaneId> = actual.iter().map(|(id, _)| *id).collect();
    let expected_ids: Vec<PaneId> = expected.iter().map(|(id, _)| p(*id)).collect();
    assert_eq!(ids, expected_ids, "rects() order");
    for ((id, a), (_, e)) in actual.iter().zip(expected) {
        assert!(
            close_to(*a, *e),
            "pane {}: got {a:?}, expected {e:?}",
            id.get()
        );
    }
}
