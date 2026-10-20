#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DropKeys {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropOp {
    Copy,
    Move,
    Link,
}

#[must_use]
pub fn drop_op(same_volume: bool, keys: DropKeys) -> DropOp {
    if keys.alt || (keys.ctrl && keys.shift) {
        DropOp::Link
    } else if keys.ctrl {
        DropOp::Copy
    } else if keys.shift || same_volume {
        DropOp::Move
    } else {
        DropOp::Copy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_truth_table() {
        use DropOp::{Copy, Link, Move};

        let cases = [
            (false, false, false, false, Copy),
            (false, true, false, false, Copy),
            (false, false, true, false, Move),
            (false, true, true, false, Link),
            (false, false, false, true, Link),
            (false, true, false, true, Link),
            (false, false, true, true, Link),
            (false, true, true, true, Link),
            (true, false, false, false, Move),
            (true, true, false, false, Copy),
            (true, false, true, false, Move),
            (true, true, true, false, Link),
            (true, false, false, true, Link),
            (true, true, false, true, Link),
            (true, false, true, true, Link),
            (true, true, true, true, Link),
        ];
        for (same_volume, ctrl, shift, alt, expected) in cases {
            let keys = DropKeys { ctrl, shift, alt };
            assert_eq!(
                drop_op(same_volume, keys),
                expected,
                "{same_volume}: {keys:?}"
            );
        }
        assert_eq!(
            DropKeys::default(),
            DropKeys {
                ctrl: false,
                shift: false,
                alt: false
            }
        );
    }
}
