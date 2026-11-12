//! "Show bytes here" on a line over 1 MiB (V41d, D18): the pair reopens in the hex body at the
//! line's byte offset, and Show text comes back to the line. Text and large pairs share it.

use std::rc::Rc;

use gpui_kit::{Stateful, WeakEntity};
use navo_l10n::tr;

use super::super::copy::Note;
use super::super::file::FileView;
use super::super::hex::Anchor as HexAnchor;
use super::super::large::Anchor as LineAnchor;
use super::super::{FileScreen, FileState};
use crate::commands::compare::ShowBytesHere;
use crate::compare_diff::{LongHit, OnBytesHere, PairKind, PickSide, is_long};
use crate::prelude::*;
use crate::window_view::ScreenOwner;

/// Where a switch lands: a byte of the hex body, or a line of the large body.
#[derive(Debug, Clone, Copy)]
pub(in crate::window_view::compare_screen) enum At {
    Hex(HexAnchor),
    Line(LineAnchor),
}

/// The click handler of a "Show bytes here" button: it opens the pair in Bytes at the row's line.
pub(in crate::window_view::compare_screen) fn on_bytes_here(
    weak: &WeakEntity<ScreenOwner>,
) -> OnBytesHere {
    let weak = weak.clone();
    Rc::new(move |hit, _, cx| {
        let _ = weak.update(cx, |view, cx| view.show_bytes_here(hit, cx));
    })
}

/// The hex anchor for a line that starts at `offset` on the left (`left`) or right file.
fn anchor(left: bool, offset: u64) -> HexAnchor {
    match left {
        true => HexAnchor::Left(offset),
        false => HexAnchor::Right(offset),
    }
}

/// `hit` narrowed to the row's line on one side, to ask whether that line is the long one.
fn one_sided(left: bool, line: u32) -> LongHit {
    match left {
        true => LongHit {
            left: Some(line),
            right: None,
        },
        false => LongHit {
            left: None,
            right: Some(line),
        },
    }
}

impl FileScreen {
    /// The line the row names: the side whose line is over 1 MiB when only that side's is, else
    /// the left one (both long, or a one-sided line, keep the left). `long` says whether the line
    /// a one-sided hit names is over the limit on this pair.
    fn line_of(hit: LongHit, long: &dyn Fn(LongHit) -> bool) -> Option<(bool, u32)> {
        match (hit.left, hit.right) {
            (Some(left), Some(right)) => {
                let only_right = !long(one_sided(true, left)) && long(one_sided(false, right));
                match only_right {
                    true => Some((false, right)),
                    false => Some((true, left)),
                }
            }
            (Some(line), None) => Some((true, line)),
            (None, Some(line)) => Some((false, line)),
            (None, None) => None,
        }
    }

    /// Shows the place a landed switch asked for: a byte in the hex body or in the text view's
    /// left file, or a line of the large body.
    pub(in crate::window_view::compare_screen) fn land(&mut self, at: At) {
        match (at, self.hex.as_mut(), self.view.as_mut()) {
            (At::Hex(at), Some(hex), _) => hex.seek(at),
            (At::Hex(HexAnchor::Left(offset)), None, Some(view)) => view.seek_offset(offset),
            (At::Line(line), ..) => {
                if let Some(large) = self.large.as_mut() {
                    large.seek_line(line);
                }
            }
            _ => {}
        }
    }

    /// Notes `hit` as the menu's line, when one of its lines in this pair is over 1 MiB: the
    /// text pair's lines read the view's text, a Large pair's read its line cache (V41e).
    pub(in crate::window_view::compare_screen) fn aim_at(&mut self, hit: LongHit) {
        self.switch.aim = match self.view.as_ref() {
            Some(view) => long_in(view, hit),
            None => self.large.as_ref().and_then(|large| large.long_hit(hit)),
        };
    }

    /// Notes the aim for the keys' command: on a text pair the line under the pick cursor, on a
    /// Large pair the first line over 1 MiB in view, left side first (V41e); nothing else.
    pub(in crate::window_view::compare_screen) fn aim_from_cursor(&mut self) {
        if self.view.is_none() {
            self.switch.aim = self.large.as_ref().and_then(|large| large.long_in_view());
            return;
        }
        let spot = self
            .view
            .as_ref()
            .and_then(|view| self.pick.cursor_spot(view.text()));
        let hit = spot.map(|spot| match spot.side {
            PickSide::Left => LongHit {
                left: Some(spot.line),
                right: None,
            },
            PickSide::Right => LongHit {
                left: None,
                right: Some(spot.line),
            },
        });
        self.switch.aim = hit.and_then(|hit| long_in(self.view.as_ref()?, hit));
    }

    /// Where Bytes opens for `hit`, and the large line Show text returns to (none for a text
    /// pair, which returns by the hex body's offset). `None` when the pair has no offset for it.
    fn here(&self, hit: LongHit) -> Option<(HexAnchor, Option<LineAnchor>)> {
        if let (
            Some(view),
            FileState::Opened {
                kind: PairKind::Text,
                ..
            },
        ) = (self.view.as_ref(), &self.state)
        {
            long_in(view, hit)?;
            let (left, line) = Self::line_of(hit, &|one| long_in(view, one).is_some())?;
            let text = view.text();
            let starts = if left {
                &text.left_starts
            } else {
                &text.right_starts
            };
            return Some((anchor(left, *starts.get(line as usize)?), None));
        }
        let large = self.large.as_ref()?;
        let (left, line) = Self::line_of(hit, &|one| large.long_hit(one).is_some())?;
        let at = u64::from(line);
        let line = if left {
            LineAnchor::Left(at)
        } else {
            LineAnchor::Right(at)
        };
        let offset = large.start_of(line)?;
        Some((anchor(left, offset), Some(line)))
    }
}

/// `hit` when one of its lines in `view` is over 1 MiB.
fn long_in(view: &FileView, hit: LongHit) -> Option<LongHit> {
    let text = view.text();
    let long = |lines: &[String], at: Option<u32>| {
        at.and_then(|at| lines.get(at as usize))
            .is_some_and(|line| is_long(line))
    };
    (long(&text.left, hit.left) || long(&text.right, hit.right)).then_some(hit)
}

impl ScreenOwner {
    /// Binds Show bytes here on the file diff's `root`: it acts on the menu's line, else the
    /// cursor's; with neither a long line it says so in the compare notice (V41e).
    pub(in crate::window_view::compare_screen) fn here_actions(
        root: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &ShowBytesHere, _, cx| {
            let Some(file) = view.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
                return;
            };
            if file.switch.aim.is_none() {
                file.aim_from_cursor();
            }
            let hit = file.switch.aim.take();
            match hit {
                Some(hit) => view.show_bytes_here(hit, cx),
                None => {
                    view.compare.diff.copy.note = Some(Note {
                        text: tr("compare-bytes-here-none").into(),
                        undo: false,
                        error: true,
                    });
                    cx.notify();
                }
            }
        }))
    }

    /// Reopens the pair as bytes at the line `hit` names, keeping the old body drawn until the
    /// hex body has rows. A switch in flight, or a pair already in bytes, ignores it.
    fn show_bytes_here(&mut self, hit: LongHit, cx: &mut Context<Self>) {
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return;
        };
        if file.switch.is_busy() {
            return;
        }
        let Some((at, back)) = file.here(hit) else {
            return;
        };
        if file.begin_switch(true, At::Hex(at)) {
            file.switch.back = back;
        }
        cx.notify();
    }
}
