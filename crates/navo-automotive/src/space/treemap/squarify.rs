use super::Rect;

pub fn squarify(values: &[u64], bounds: Rect) -> Vec<Rect> {
    let empty = Rect::new(bounds.x, bounds.y, 0.0, 0.0);
    let mut out = vec![empty; values.len()];
    let live: Vec<usize> = (0..values.len()).filter(|&at| values[at] > 0).collect();
    let total: f64 = live.iter().map(|&at| values[at] as f64).sum();
    if live.is_empty() || bounds.area() <= 0.0 {
        return out;
    }
    let scale = bounds.area() / total;
    let areas: Vec<f64> = live.iter().map(|&at| values[at] as f64 * scale).collect();
    let mut free = bounds;
    let mut start = 0;
    while start < areas.len() {
        let end = row_end(&areas, start, free.w.min(free.h));
        let last = end == areas.len();
        let sum: f64 = areas[start..end].iter().sum();
        let placed = place(&areas[start..end], sum, &mut free, last);
        for (rect, &at) in placed.into_iter().zip(&live[start..end]) {
            out[at] = rect;
        }
        start = end;
    }
    out
}

fn row_end(areas: &[f64], start: usize, side: f64) -> usize {
    let mut sum = areas[start];
    let (mut low, mut high) = (areas[start], areas[start]);
    let mut worst = worst_ratio(sum, low, high, side);
    let mut end = start + 1;
    while end < areas.len() {
        let next = areas[end];
        let (next_low, next_high) = (low.min(next), high.max(next));
        let next_worst = worst_ratio(sum + next, next_low, next_high, side);
        if next_worst > worst {
            break;
        }
        (sum, low, high, worst) = (sum + next, next_low, next_high, next_worst);
        end += 1;
    }
    end
}

fn worst_ratio(sum: f64, low: f64, high: f64, side: f64) -> f64 {
    let square = side * side;
    (square * high / (sum * sum)).max(sum * sum / (square * low))
}

fn place(row: &[f64], sum: f64, free: &mut Rect, last: bool) -> Vec<Rect> {
    let column = free.w >= free.h;
    let length = if column { free.h } else { free.w };
    let thickness = match (last, column) {
        (true, true) => free.w,
        (true, false) => free.h,
        (false, _) => sum / length,
    };
    let mut rects = Vec::with_capacity(row.len());
    let mut offset = 0.0;
    for (at, &area) in row.iter().enumerate() {
        let span = if at + 1 == row.len() {
            length - offset
        } else {
            area / thickness
        };
        rects.push(if column {
            Rect::new(free.x, free.y + offset, thickness, span)
        } else {
            Rect::new(free.x + offset, free.y, span, thickness)
        });
        offset += span;
    }
    if column {
        free.x += thickness;
        free.w -= thickness;
    } else {
        free.y += thickness;
        free.h -= thickness;
    }
    rects
}
