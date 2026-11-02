use gpui_kit::{Bounds, Pixels, Point, point};

use super::place::Side;

/// True while the pointer, moving from `from` to `to`, heads for `target`: `to` lies in the
/// triangle between `from` and the near edge of `target` (the "safe triangle").
pub(super) fn is_aiming(
    from: Point<Pixels>,
    to: Point<Pixels>,
    target: Bounds<Pixels>,
    side: Side,
) -> bool {
    if from == to {
        return false;
    }
    let edge = match side {
        Side::Right => target.left(),
        Side::Left => target.right(),
    };
    let top = point(edge, target.top());
    let bottom = point(edge, target.bottom());
    is_inside(to, [from, top, bottom])
}

fn is_inside(at: Point<Pixels>, [a, b, c]: [Point<Pixels>; 3]) -> bool {
    let cross = |o: Point<Pixels>, u: Point<Pixels>| {
        f32::from(u.x - o.x) * f32::from(at.y - o.y) - f32::from(u.y - o.y) * f32::from(at.x - o.x)
    };
    let sides = [cross(a, b), cross(b, c), cross(c, a)];
    let has_negative = sides.iter().any(|side| *side < 0.);
    let has_positive = sides.iter().any(|side| *side > 0.);
    !(has_negative && has_positive)
}
