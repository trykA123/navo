use super::tree::{Axis, Node};
use super::{Rect, SplitInfo, SplitPath};
use crate::{Direction, PaneId};

const EPS: f64 = 1e-9;
const PERMILLE: f64 = 1000.0;

pub(super) fn rects(node: &Node, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
    match node {
        Node::Leaf(pane) => out.push((*pane, area)),
        Node::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let (a, b) = divide(area, *axis, f64::from(*ratio) / PERMILLE);
            rects(first, a, out);
            rects(second, b, out);
        }
    }
}

pub(super) fn splits(node: &Node, area: Rect, out: &mut Vec<SplitInfo>) {
    let Node::Split {
        axis,
        ratio,
        first,
        second,
    } = node
    else {
        return;
    };
    out.push(SplitInfo {
        path: SplitPath(Vec::new()),
        axis: *axis,
        ratio: *ratio,
        rect: area,
    });
    let (a, b) = divide(area, *axis, f64::from(*ratio) / PERMILLE);
    for (is_second, child, rect) in [(false, first, a), (true, second, b)] {
        let start = out.len();
        splits(child, rect, out);
        for split in &mut out[start..] {
            split.path.0.insert(0, is_second);
        }
    }
}

fn divide(area: Rect, axis: Axis, share: f64) -> (Rect, Rect) {
    match axis {
        Axis::SideBySide => {
            let w = area.width * share;
            let first = Rect { width: w, ..area };
            let second = Rect {
                x: area.x + w,
                width: area.width - w,
                ..area
            };
            (first, second)
        }
        Axis::Stacked => {
            let h = area.height * share;
            let first = Rect { height: h, ..area };
            let second = Rect {
                y: area.y + h,
                height: area.height - h,
                ..area
            };
            (first, second)
        }
    }
}

struct Candidate {
    pane: PaneId,
    overlap: f64,
    start: f64,
}

pub(super) fn neighbour(
    all: &[(PaneId, Rect)],
    pane: PaneId,
    direction: Direction,
) -> Option<PaneId> {
    let (_, from) = all.iter().find(|(id, _)| *id == pane)?;
    all.iter()
        .filter(|(id, _)| *id != pane)
        .filter_map(|(id, to)| candidate(*from, *id, *to, direction))
        .reduce(|best, c| if better(&c, &best) { c } else { best })
        .map(|c| c.pane)
}

fn candidate(from: Rect, pane: PaneId, to: Rect, direction: Direction) -> Option<Candidate> {
    let touches = match direction {
        Direction::Right => close(to.x, from.x + from.width),
        Direction::Left => close(to.x + to.width, from.x),
        Direction::Down => close(to.y, from.y + from.height),
        Direction::Up => close(to.y + to.height, from.y),
    };
    let (start, overlap) = if direction.is_horizontal() {
        (to.y, span_overlap(from.y, from.height, to.y, to.height))
    } else {
        (to.x, span_overlap(from.x, from.width, to.x, to.width))
    };
    (touches && overlap > EPS).then_some(Candidate {
        pane,
        overlap,
        start,
    })
}

fn better(c: &Candidate, best: &Candidate) -> bool {
    if close(c.overlap, best.overlap) {
        c.start < best.start - EPS
    } else {
        c.overlap > best.overlap
    }
}

fn span_overlap(a: f64, a_len: f64, b: f64, b_len: f64) -> f64 {
    (a + a_len).min(b + b_len) - a.max(b)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}
