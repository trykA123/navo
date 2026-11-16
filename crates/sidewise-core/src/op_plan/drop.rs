#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DropKeys {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

const MK_RBUTTON: u32 = 0x02;
const MK_SHIFT: u32 = 0x04;
const MK_CONTROL: u32 = 0x08;
const MK_ALT: u32 = 0x20;

impl DropKeys {
    #[must_use]
    pub fn from_key_state(key_state: u32) -> Self {
        Self {
            ctrl: key_state & MK_CONTROL != 0,
            shift: key_state & MK_SHIFT != 0,
            alt: key_state & MK_ALT != 0,
        }
    }
}

#[must_use]
pub fn right_button_drag(key_state: u32) -> bool {
    key_state & MK_RBUTTON != 0
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

    #[test]
    fn key_state_maps_to_drop_keys() {
        const LBUTTON: u32 = 0x01;
        const RBUTTON: u32 = 0x02;
        const SHIFT: u32 = 0x04;
        const CONTROL: u32 = 0x08;
        const MBUTTON: u32 = 0x10;
        const ALT: u32 = 0x20;

        let keys = |ctrl, shift, alt| DropKeys { ctrl, shift, alt };
        let cases = [
            (0, keys(false, false, false), false),
            (LBUTTON, keys(false, false, false), false),
            (CONTROL, keys(true, false, false), false),
            (SHIFT, keys(false, true, false), false),
            (ALT, keys(false, false, true), false),
            (RBUTTON, keys(false, false, false), true),
            (CONTROL | SHIFT, keys(true, true, false), false),
            (LBUTTON | ALT, keys(false, false, true), false),
            (RBUTTON | CONTROL, keys(true, false, false), true),
            (RBUTTON | SHIFT | ALT, keys(false, true, true), true),
            (
                CONTROL | SHIFT | ALT | RBUTTON,
                keys(true, true, true),
                true,
            ),
            (MBUTTON | LBUTTON, keys(false, false, false), false),
            (u32::MAX, keys(true, true, true), true),
        ];
        for (state, expected, right) in cases {
            assert_eq!(DropKeys::from_key_state(state), expected, "{state:#x}");
            assert_eq!(right_button_drag(state), right, "{state:#x}");
        }
    }
}
