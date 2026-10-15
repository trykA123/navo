use crate::{Layout, PaneId, Rect};

mod props;
mod unit;

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
