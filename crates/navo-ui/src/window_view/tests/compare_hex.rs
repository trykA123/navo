//! The hex body of the file diff (V39): span-safe rows, background fetched bytes, region
//! steps past f32 precision, the user's alignment and the context-menu walk entries.

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, px};
use navo_core::Location;
use navo_core::paging::{ByteSource, LineIndex, hex_row};

use super::compare_screen::Rig;
use super::compare_screen::rig_with_files;
use super::settle;
use crate::compare_diff::{ByteProgress, ByteRegionView, DiffUpdate, HexAlign, PairKind};
use crate::window_view::compare_screen::HexProbe;
use crate::window_view::compare_screen::menu::AlignContent;
use crate::window_view::{FileHooks, Paged};

/// A byte source with a fixed length; only the bytes it holds are readable.
struct Memory {
    bytes: Arc<Vec<u8>>,
    len: u64,
}

impl ByteSource for Memory {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let len = self.bytes.len();
        let start = usize::try_from(offset).unwrap_or(usize::MAX).min(len);
        let end = start.saturating_add(buf.len()).min(len);
        let count = end - start;
        buf[..count].copy_from_slice(&self.bytes[start..end]);
        Ok(count)
    }
}

/// A gate the paged hook waits on, so a test can refresh before an open lands.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    ready: Condvar,
}

impl Gate {
    /// Blocks until `release`.
    fn wait(&self) {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        while !*open {
            open = self
                .ready
                .wait(open)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Lets every waiter continue.
    fn release(&self) {
        *self.open.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.ready.notify_all();
    }
}

/// A paged side over `bytes`, listed as `len` bytes long.
fn paged(bytes: Arc<Vec<u8>>, len: u64) -> Paged {
    Paged {
        source: Arc::new(Memory { bytes, len }),
        reopen: Arc::new(|| Err("not reopened in this test".to_owned())),
        index: Arc::new(|_: &AtomicBool, _: &mut dyn FnMut(&LineIndex)| Ok(None)),
        summary: None,
        problem: None,
        editable: false,
        stamp: None,
    }
}

/// A compare screen whose pair sides open from `left` and `right`.
fn hex_rig_len(
    cx: &mut TestAppContext,
    left: Vec<u8>,
    right: Vec<u8>,
    (left_len, right_len): (u64, u64),
) -> Rig {
    let next = Arc::new(AtomicUsize::new(0));
    let sides = [Arc::new(left), Arc::new(right)];
    let lens = [left_len, right_len];
    let files = FileHooks {
        paged: Arc::new(move |_: &Location| {
            let at = next.fetch_add(1, Ordering::Relaxed) % 2;
            Ok(paged(Arc::clone(&sides[at]), lens[at]))
        }),
        ..FileHooks::none()
    };
    rig_with_files(cx, px(1000.), files)
}

/// A compare screen whose paged sides wait for `gate`.
fn hex_rig_gated(cx: &mut TestAppContext, gate: Arc<Gate>) -> Rig {
    let next = Arc::new(AtomicUsize::new(0));
    let files = FileHooks {
        paged: Arc::new(move |_: &Location| {
            gate.wait();
            let _ = next.fetch_add(1, Ordering::Relaxed);
            Ok(paged(Arc::new(vec![0x11; 64]), 64))
        }),
        ..FileHooks::none()
    };
    rig_with_files(cx, px(1000.), files)
}

/// A compare screen whose first pair opens from `first`; every later open waits for `gate`,
/// then gives `second`, or fails when `second` is `None`.
fn hex_rig_reopen(
    cx: &mut TestAppContext,
    gate: Arc<Gate>,
    first: (Vec<u8>, Vec<u8>),
    second: Option<(Vec<u8>, Vec<u8>)>,
) -> Rig {
    let next = Arc::new(AtomicUsize::new(0));
    let sides = [first, second.clone().unwrap_or_default()];
    let files = FileHooks {
        paged: Arc::new(move |_: &Location| {
            let call = next.fetch_add(1, Ordering::Relaxed);
            if call >= 2 {
                gate.wait();
            }
            if call >= 2 && second.is_none() {
                return Err("the side cannot reopen".to_owned());
            }
            let pair = &sides[call / 2];
            let bytes = if call.is_multiple_of(2) {
                &pair.0
            } else {
                &pair.1
            };
            let len = u64::try_from(bytes.len()).unwrap_or(0);
            Ok(paged(Arc::new(bytes.clone()), len))
        }),
        ..FileHooks::none()
    };
    rig_with_files(cx, px(1000.), files)
}

/// Refreshes the open binary pair and delivers its second `Opened`; the reopen is started but
/// never run here, as the test scheduler would block on the gate.
fn refresh_hex(cx: &mut TestAppContext, rig: &Rig) {
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    let sink = rig.diff.pair_sink();
    sink(DiffUpdate::Opened {
        request: 2,
        kind: PairKind::Binary,
        text: None,
    });
    rig.view.update(cx, |view, cx| view.wake(cx));
}

/// Runs queued tasks one at a time until the hex body has opened its sides or failed, so the
/// rows read for the new pair are still queued behind it.
fn until_reopened(cx: &mut TestAppContext, rig: &Rig) {
    for _ in 0..2000 {
        if !cx.executor().tick() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if probe(cx, rig).ready {
            return;
        }
    }
}

/// A compare screen over two equally long sides.
pub(super) fn hex_rig(cx: &mut TestAppContext, left: Vec<u8>, right: Vec<u8>) -> Rig {
    let left_len = u64::try_from(left.len()).unwrap_or(0);
    let right_len = u64::try_from(right.len()).unwrap_or(0);
    hex_rig_len(cx, left, right, (left_len, right_len))
}

/// Renders a frame and lets the deferred background row read land.
pub(super) fn frame(cx: &mut TestAppContext, rig: &Rig) {
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    settle(cx, &rig.view);
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

/// Opens the selected row as a binary pair and sends one byte snapshot per region.
fn open_hex(
    cx: &mut TestAppContext,
    rig: &Rig,
    regions: &[(Range<u64>, Range<u64>)],
    align: HexAlign,
) {
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    rig.push(
        cx,
        &sink,
        DiffUpdate::Opened {
            request: 1,
            kind: PairKind::Binary,
            text: None,
        },
    );
    for (left, right) in regions {
        rig.push(
            cx,
            &sink,
            DiffUpdate::Bytes {
                request: 1,
                align,
                run: 1,
                regions: vec![ByteRegionView {
                    left: left.clone(),
                    right: right.clone(),
                    changed: left
                        .end
                        .saturating_sub(left.start)
                        .max(right.end.saturating_sub(right.start)),
                }],
                progress: Some(ByteProgress {
                    done: 1,
                    total: 1,
                    coarse: false,
                }),
            },
        );
    }
    frame(cx, rig);
}

/// The current region, visible rows, row count, alignment and gap rows.
pub(super) fn probe(cx: &mut TestAppContext, rig: &Rig) -> HexProbe {
    rig.view
        .update(cx, |view, _| view.file_hex_probe())
        .expect("the binary pair shows a hex body")
}

/// A compare screen whose paged hook counts its calls; with `fails` every open fails.
fn hex_rig_counted(cx: &mut TestAppContext, fails: bool) -> (Rig, Arc<AtomicUsize>) {
    let opens = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&opens);
    let files = FileHooks {
        paged: Arc::new(move |_: &Location| {
            counted.fetch_add(1, Ordering::Relaxed);
            if fails {
                return Err("the side cannot open".to_owned());
            }
            Ok(paged(Arc::new(vec![0x11; 0x400]), 0x400))
        }),
        ..FileHooks::none()
    };
    (rig_with_files(cx, px(1000.), files), opens)
}

/// Bytes whose value depends on their offset, so a row at the wrong offset shows wrong bytes.
fn patterned(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|at| (at * 7 % 251) as u8 ^ seed).collect()
}

/// One byte snapshot holding one region of pair `request`.
fn snapshot(
    (request, run): (u64, u64),
    align: HexAlign,
    (left, right): (Range<u64>, Range<u64>),
) -> DiffUpdate {
    let changed = (left.end - left.start).max(right.end - right.start);
    DiffUpdate::Bytes {
        request,
        align,
        run,
        regions: vec![ByteRegionView {
            left,
            right,
            changed,
        }],
        progress: None,
    }
}

/// Draws one frame without letting a background read land.
fn draw_only(cx: &mut TestAppContext, rig: &Rig) {
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

/// Whether the last frame drew element `id` for side slot `index`.
fn drawn(cx: &mut TestAppContext, rig: &Rig, id: &'static str, index: usize) -> bool {
    cx.update_window(rig.window, |_, window, _| {
        window.try_find((id, index)).is_some()
    })
    .expect("window")
}

/// The label the last frame drew for side slot `index`: offset, hex and ASCII.
fn row_label(cx: &mut TestAppContext, rig: &Rig, index: usize) -> String {
    cx.update_window(rig.window, |_, window, _| {
        window
            .find(("paged-row", index))
            .label()
            .unwrap_or_default()
            .to_owned()
    })
    .expect("window")
}

/// The label of a full row of `bytes` at offset `at`.
fn label_at(bytes: &[u8], at: usize) -> String {
    let row = hex_row(at as u64, &bytes[at..at + 16], false);
    format!("{}  {}  {}", row.offset, row.hex, row.ascii)
}

/// How many rows the window shows.
pub(super) fn visible(cx: &mut TestAppContext, rig: &Rig) -> usize {
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.read(cx).file_hex_visible(window, cx)
    })
    .expect("window")
}

#[gpui_kit::test]
fn compare_screen_hex_next_and_previous_region_walk_and_scroll(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x4000], vec![0x22; 0x4000]);
    open_hex(
        cx,
        &rig,
        &[
            (0x1000..0x1004, 0x1000..0x1004),
            (0x2000..0x2004, 0x2000..0x2004),
            (0x3000..0x3004, 0x3000..0x3004),
        ],
        HexAlign::Offset,
    );
    let half = visible(cx, &rig) / 2;
    let first = probe(cx, &rig);
    assert_eq!(first.current, None);
    assert_eq!(first.rows, 0x400);
    assert_eq!(
        first.built.start, 0,
        "nothing scrolls before the first step"
    );
    rig.press(cx, "f7");
    let at_first = probe(cx, &rig);
    assert_eq!(at_first.current, Some(0));
    assert_eq!(
        at_first.built.start,
        0x100 - half,
        "the first region is centred"
    );
    rig.press(cx, "alt-down");
    let at_second = probe(cx, &rig);
    assert_eq!(at_second.current, Some(1));
    assert_eq!(
        at_second.built.start,
        0x200 - half,
        "the second region is centred"
    );
    rig.press(cx, "f7");
    assert_eq!(probe(cx, &rig).current, Some(2));
    rig.press(cx, "alt-down");
    assert_eq!(probe(cx, &rig).current, Some(2));
    rig.press(cx, "alt-up");
    let back = probe(cx, &rig);
    assert_eq!(back.current, Some(1));
    assert_eq!(back.built.start, 0x200 - half, "stepping back scrolls back");
}

#[gpui_kit::test]
fn compare_screen_hex_content_alignment_puts_a_gap_where_a_side_inserts(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 16], vec![0x22; 24]);
    open_hex(cx, &rig, &[(8..8, 8..16)], HexAlign::Content);
    let shown = probe(cx, &rig);
    assert_eq!(shown.align, "content");
    assert_eq!(shown.rows, 3);
    assert_eq!(&shown.gaps[..3], [false, true, false]);
}

#[gpui_kit::test]
fn compare_screen_hex_rows_carry_their_own_byte_count(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 16], vec![0x22; 24]);
    open_hex(cx, &rig, &[(8..8, 8..16)], HexAlign::Content);
    let shown = probe(cx, &rig);
    assert!(shown.loaded[0], "the first row's bytes were fetched");
    let row = shown.left_hex[0].as_deref().expect("the left row");
    assert_eq!(
        row.split(' ').filter(|part| !part.is_empty()).count(),
        8,
        "the source has 16 bytes but the span holds 8: {row}"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_keeps_the_user_alignment_when_the_open_lands(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 64], vec![0x22; 64]);
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    sink(DiffUpdate::Opened {
        request: 1,
        kind: PairKind::Binary,
        text: None,
    });
    rig.view.update(cx, |view, cx| view.wake(cx));
    rig.dispatch(cx, &AlignContent);
    settle(cx, &rig.view);
    assert_eq!(probe(cx, &rig).align, "content");
}

#[gpui_kit::test]
fn compare_screen_hex_scrolls_past_f32_row_precision(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let len = 0x1_0000_0000;
    let rig = hex_rig_len(cx, vec![0x11; 16], vec![0x22; 16], (len, len));
    let target = (1usize << 24) + 1;
    let at = target as u64 * 16;
    open_hex(cx, &rig, &[(at..at + 16, at..at + 16)], HexAlign::Offset);
    let half = visible(cx, &rig) / 2;
    rig.press(cx, "f7");
    assert_eq!(
        probe(cx, &rig).built.start,
        target - half,
        "the region's row is a count, not an f32 pixel"
    );
    let page = visible(cx, &rig);
    let top = rig
        .view
        .update(cx, |view, _| view.file_hex_wheel(half as f64, page))
        .expect("the hex body");
    assert_eq!(top, target, "row 2^24 + 1 has no f32 value");
    assert_eq!(probe(cx, &rig).built.start, target);
}

#[gpui_kit::test]
fn compare_screen_hex_menu_offers_next_difference(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 64], vec![0x22; 64]);
    open_hex(cx, &rig, &[(0x10..0x14, 0x10..0x14)], HexAlign::Offset);
    rig.press(cx, "shift-f10");
    rig.press(cx, "down");
    rig.press(cx, "enter");
    assert_eq!(
        probe(cx, &rig).current,
        Some(0),
        "the first entry is Next difference"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_drag_reaches_the_middle_and_the_end(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let len = 0x1_0000_0000;
    let rig = hex_rig_len(cx, vec![0x11; 16], vec![0x22; 16], (len, len));
    open_hex(cx, &rig, &[], HexAlign::Offset);
    let max = 0x1_0000_0000usize / 16 - 24;
    let drag = |rig: &Rig, cx: &mut TestAppContext, fraction: f64| {
        rig.view
            .update(cx, |view, _| view.file_hex_set_drag(fraction));
        cx.update_window(rig.window, |_, window, cx| {
            rig.view
                .update(cx, |view, cx| view.request_hex_rows(window, cx));
        })
        .expect("window");
        probe(cx, rig).built.start
    };
    let middle = drag(&rig, cx, 0.5);
    assert!(
        (middle as isize - (max / 2) as isize).abs() <= 1,
        "middle: {middle}"
    );
    let end = drag(&rig, cx, 1.0);
    assert_eq!(end, max, "the thumb reaches the last page");
}

#[gpui_kit::test]
fn compare_screen_hex_keeps_fractional_wheel_rows(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x400], vec![0x22; 0x400]);
    open_hex(cx, &rig, &[], HexAlign::Offset);
    let page = visible(cx, &rig);
    let top = |rig: &Rig, cx: &mut TestAppContext, rows| {
        rig.view
            .update(cx, |view, _| view.file_hex_wheel(rows, page))
            .expect("the hex body")
    };
    assert_eq!(top(&rig, cx, 0.4), 0);
    assert_eq!(top(&rig, cx, 0.4), 0);
    assert_eq!(top(&rig, cx, 0.2), 1);
}

#[gpui_kit::test]
fn compare_screen_hex_unfetched_rows_are_not_hatched(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 16], vec![0x22; 24]);
    open_hex(cx, &rig, &[(8..8, 8..16)], HexAlign::Content);
    rig.view.update(cx, |view, _| view.file_hex_forget_rows());
    draw_only(cx, &rig);
    assert!(!probe(cx, &rig).loaded[0], "the row is not fetched");
    assert!(
        drawn(cx, &rig, "compare-hex-blank", 0),
        "an unfetched side draws a blank cell"
    );
    assert!(
        !drawn(cx, &rig, "compare-hex-gap", 0),
        "an unfetched side is not a gap"
    );
    assert!(
        drawn(cx, &rig, "compare-hex-gap", 2),
        "the side with no bytes in row 1 is a gap"
    );
    assert!(
        !drawn(cx, &rig, "compare-hex-blank", 2),
        "a real gap is not a blank cell"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_drops_regions_from_an_old_alignment_run(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 64], vec![0x22; 64]);
    open_hex(cx, &rig, &[(0x10..0x14, 0x10..0x14)], HexAlign::Offset);
    let sink = rig.diff.pair_sink();
    let send = |cx: &mut TestAppContext, run: u64, at: u64, align| {
        rig.push(
            cx,
            &sink,
            DiffUpdate::Bytes {
                request: 1,
                align,
                run,
                regions: vec![ByteRegionView {
                    left: at..at + 4,
                    right: at..at + 4,
                    changed: 4,
                }],
                progress: None,
            },
        );
    };
    rig.dispatch(cx, &AlignContent);
    send(cx, 2, 0x20, HexAlign::Content);
    rig.dispatch(cx, &crate::window_view::compare_screen::menu::AlignOffset);
    send(cx, 3, 0x10, HexAlign::Offset);
    send(cx, 2, 0x20, HexAlign::Offset);
    assert_eq!(probe(cx, &rig).regions, 1, "old runs are not replayed");
}

#[gpui_kit::test]
fn compare_screen_hex_refresh_drops_a_stale_open(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let gate = Arc::new(Gate::default());
    let rig = hex_rig_gated(cx, Arc::clone(&gate));
    rig.press(cx, "enter");
    let sink = rig.diff.pair_sink();
    sink(DiffUpdate::Opened {
        request: 1,
        kind: PairKind::Binary,
        text: None,
    });
    rig.view.update(cx, |view, cx| view.wake(cx));
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    gate.release();
    settle(cx, &rig.view);
    assert!(
        !probe(cx, &rig).ready,
        "an open started before the refresh must not install"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_content_add_keeps_loaded_rows(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (left, right) = (patterned(0x4000, 0), patterned(0x4010, 0x55));
    let rig = hex_rig(cx, left.clone(), right.clone());
    open_hex(cx, &rig, &[(0x40..0x40, 0x40..0x48)], HexAlign::Content);
    assert_eq!(
        row_label(cx, &rig, 20),
        label_at(&left, 0x90),
        "row 10 before"
    );
    let sink = rig.diff.pair_sink();
    sink(snapshot(
        (1, 1),
        HexAlign::Content,
        (0x80..0x80, 0x88..0x90),
    ));
    rig.view.update(cx, |view, cx| view.wake(cx));
    draw_only(cx, &rig);
    assert!(
        probe(cx, &rig).loaded[0],
        "bytes already on screen stay until the new fetch lands"
    );
    for slot in 0..40 {
        assert!(
            !drawn(cx, &rig, "compare-hex-blank", slot),
            "side slot {slot} went blank while its bytes are on the way"
        );
    }
    assert_eq!(
        row_label(cx, &rig, 20),
        label_at(&left, 0x90),
        "until the fetch lands the screen keeps the map its bytes belong to"
    );
    frame(cx, &rig);
    assert!(
        drawn(cx, &rig, "compare-hex-gap", 18),
        "row 9 is the new insertion, with no left bytes"
    );
    assert_eq!(
        row_label(cx, &rig, 20),
        label_at(&left, 0x80),
        "row 10 moved: it shows the bytes of its new span"
    );
    assert_eq!(
        row_label(cx, &rig, 21),
        label_at(&right, 0x90),
        "the right side of row 10 shifted by half a row"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_refresh_keeps_the_user_alignment(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 64], vec![0x22; 64]);
    open_hex(cx, &rig, &[], HexAlign::Offset);
    rig.dispatch(cx, &AlignContent);
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    let sink = rig.diff.pair_sink();
    rig.push(
        cx,
        &sink,
        DiffUpdate::Opened {
            request: 2,
            kind: PairKind::Binary,
            text: None,
        },
    );
    assert_eq!(probe(cx, &rig).align, "content");
}

#[gpui_kit::test]
fn compare_screen_hex_alignment_switch_changes_the_rows(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 64], vec![0x22; 64]);
    open_hex(cx, &rig, &[], HexAlign::Offset);
    let offset = probe(cx, &rig);
    assert_eq!((offset.align, offset.rows), ("offset", 4));
    assert!(!offset.gaps[1], "offset rows have no gap inside both sides");
    rig.dispatch(cx, &AlignContent);
    let sink = rig.diff.pair_sink();
    rig.push(
        cx,
        &sink,
        snapshot((1, 2), HexAlign::Content, (0x10..0x10, 0x10..0x20)),
    );
    let content = probe(cx, &rig);
    assert_eq!((content.align, content.rows), ("content", 5));
    assert!(
        content.gaps[1],
        "the insertion is a gap row in content rows"
    );
    rig.dispatch(cx, &crate::window_view::compare_screen::menu::AlignOffset);
    let back = probe(cx, &rig);
    assert_eq!((back.align, back.rows), ("offset", 4));
    assert!(!back.gaps[1], "back in offset rows the gap is gone");
}

#[gpui_kit::test]
fn compare_screen_hex_opens_once_across_wakes(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, opens) = hex_rig_counted(cx, false);
    open_hex(cx, &rig, &[(0x10..0x14, 0x10..0x14)], HexAlign::Offset);
    assert_eq!(opens.load(Ordering::Relaxed), 2, "one open per side");
    let sink = rig.diff.pair_sink();
    rig.push(
        cx,
        &sink,
        snapshot((1, 1), HexAlign::Offset, (0x20..0x24, 0x20..0x24)),
    );
    settle(cx, &rig.view);
    settle(cx, &rig.view);
    assert_eq!(opens.load(Ordering::Relaxed), 2, "later wakes never reopen");
    let shown = probe(cx, &rig);
    assert_eq!(shown.regions, 2);
    assert!(shown.loaded[0], "a later update keeps the fetched rows");
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    rig.push(
        cx,
        &sink,
        DiffUpdate::Opened {
            request: 2,
            kind: PairKind::Binary,
            text: None,
        },
    );
    settle(cx, &rig.view);
    assert_eq!(
        opens.load(Ordering::Relaxed),
        4,
        "a refresh opens a new view once"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_failed_open_is_not_retried(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (rig, opens) = hex_rig_counted(cx, true);
    open_hex(cx, &rig, &[], HexAlign::Offset);
    assert_eq!(opens.load(Ordering::Relaxed), 2, "one try per side");
    let sink = rig.diff.pair_sink();
    rig.push(cx, &sink, snapshot((1, 1), HexAlign::Offset, (0..4, 0..4)));
    settle(cx, &rig.view);
    assert_eq!(
        opens.load(Ordering::Relaxed),
        2,
        "a failed open stays failed"
    );
    assert!(probe(cx, &rig).ready, "the failure is kept for the body");
}

#[gpui_kit::test]
fn compare_screen_hex_drops_bytes_that_arrive_before_the_reopen_asks(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 64], vec![0x22; 64]);
    open_hex(cx, &rig, &[(0x10..0x14, 0x10..0x14)], HexAlign::Offset);
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    let sink = rig.diff.pair_sink();
    sink(DiffUpdate::Opened {
        request: 2,
        kind: PairKind::Binary,
        text: None,
    });
    sink(snapshot((2, 1), HexAlign::Offset, (0x10..0x14, 0x10..0x14)));
    settle(cx, &rig.view);
    let asked = probe(cx, &rig);
    assert_eq!(asked.run, 2, "the landed open asks for a new run");
    assert_eq!(asked.regions, 0, "nothing was asked when those bytes came");
    rig.push(
        cx,
        &sink,
        snapshot((2, 2), HexAlign::Offset, (0x10..0x14, 0x10..0x14)),
    );
    assert_eq!(probe(cx, &rig).regions, 1, "the region shows once");
}

#[gpui_kit::test]
fn compare_screen_hex_alignment_toggled_during_the_reopen_keeps_its_regions(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 64], vec![0x22; 64]);
    open_hex(cx, &rig, &[], HexAlign::Offset);
    rig.view.update(cx, |view, _| view.refresh_file_screen());
    let sink = rig.diff.pair_sink();
    sink(DiffUpdate::Opened {
        request: 2,
        kind: PairKind::Binary,
        text: None,
    });
    rig.view.update(cx, |view, cx| view.wake(cx));
    rig.view
        .update(cx, |view, _| view.file_hex_set_align(HexAlign::Content));
    sink(snapshot(
        (2, 2),
        HexAlign::Content,
        (0x10..0x14, 0x10..0x14),
    ));
    rig.view.update(cx, |view, cx| view.wake(cx));
    let early = probe(cx, &rig);
    assert!(!early.ready, "the bytes arrive before the open lands");
    assert_eq!(early.regions, 1, "the toggle asked for these bytes");
    settle(cx, &rig.view);
    let shown = probe(cx, &rig);
    assert!(shown.ready, "the open landed");
    assert_eq!(shown.run, 2, "the toggle's run is the one in force");
    assert_eq!(shown.regions, 1, "its region survives the open landing");
}

#[gpui_kit::test]
fn compare_screen_hex_sorting_keeps_the_current_region(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x400], vec![0x22; 0x400]);
    open_hex(cx, &rig, &[(0x200..0x204, 0x200..0x204)], HexAlign::Offset);
    rig.press(cx, "f7");
    assert_eq!(probe(cx, &rig).current, Some(0));
    let sink = rig.diff.pair_sink();
    rig.push(
        cx,
        &sink,
        snapshot((1, 1), HexAlign::Offset, (0x10..0x14, 0x10..0x14)),
    );
    let shown = probe(cx, &rig);
    assert_eq!(shown.regions, 2);
    assert_eq!(
        shown.current,
        Some(1),
        "the region at 0x200 moved to index 1"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_refresh_keeps_the_old_rows_until_the_new_bytes_land(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (old, new) = (patterned(0x400, 0), patterned(0x400, 0x5a));
    let gate = Arc::new(Gate::default());
    let rig = hex_rig_reopen(
        cx,
        Arc::clone(&gate),
        (old.clone(), old.clone()),
        Some((new.clone(), new.clone())),
    );
    open_hex(cx, &rig, &[(0x10..0x14, 0x10..0x14)], HexAlign::Offset);
    assert_eq!(row_label(cx, &rig, 0), label_at(&old, 0), "the old bytes");
    refresh_hex(cx, &rig);
    draw_only(cx, &rig);
    assert!(!probe(cx, &rig).ready, "the reopen has not landed");
    assert!(
        drawn(cx, &rig, "paged-row", 0),
        "the body is not blank while the reopen is on its way"
    );
    assert_eq!(
        row_label(cx, &rig, 0),
        label_at(&old, 0),
        "the old rows stay drawn while the reopen is on its way"
    );
    assert!(!drawn(cx, &rig, "compare-hex-blank", 0), "no blank cell");
    assert_eq!(
        probe(cx, &rig).status,
        None,
        "no 'No changes' before the open lands"
    );
    gate.release();
    until_reopened(cx, &rig);
    draw_only(cx, &rig);
    assert!(
        drawn(cx, &rig, "paged-row", 0),
        "the body is not blank after the open lands"
    );
    assert_eq!(
        row_label(cx, &rig, 0),
        label_at(&old, 0),
        "the old rows stay drawn after the open lands, until the new bytes are read"
    );
    assert!(!drawn(cx, &rig, "compare-hex-blank", 0), "no blank cell");
    assert_eq!(
        probe(cx, &rig).status,
        None,
        "no 'No changes' while the old rows are drawn"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_refresh_shows_the_new_bytes_once_read(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let (old, new) = (patterned(0x400, 0), patterned(0x400, 0x5a));
    let gate = Arc::new(Gate::default());
    let rig = hex_rig_reopen(
        cx,
        Arc::clone(&gate),
        (old.clone(), old.clone()),
        Some((new.clone(), new.clone())),
    );
    open_hex(cx, &rig, &[], HexAlign::Offset);
    refresh_hex(cx, &rig);
    gate.release();
    until_reopened(cx, &rig);
    frame(cx, &rig);
    assert_eq!(
        row_label(cx, &rig, 0),
        label_at(&new, 0),
        "the rows show the new bytes, not the held ones"
    );
    assert_eq!(row_label(cx, &rig, 1), label_at(&new, 0), "right side too");
    assert!(probe(cx, &rig).loaded[0], "the new cache holds the bytes");
}

#[gpui_kit::test]
fn compare_screen_hex_failed_reopen_shows_the_failure_not_old_rows(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let old = patterned(0x400, 0);
    let gate = Arc::new(Gate::default());
    let rig = hex_rig_reopen(cx, Arc::clone(&gate), (old.clone(), old.clone()), None);
    open_hex(cx, &rig, &[], HexAlign::Offset);
    assert!(drawn(cx, &rig, "paged-row", 0), "the old rows are drawn");
    refresh_hex(cx, &rig);
    gate.release();
    until_reopened(cx, &rig);
    frame(cx, &rig);
    assert!(
        !drawn(cx, &rig, "paged-row", 0),
        "no old row is left on screen as if valid"
    );
    assert!(
        cx.update_window(rig.window, |_, window, _| {
            window.try_find("compare-hex-message").is_some()
        })
        .expect("window"),
        "the failure message shows"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_stepping_to_the_last_page_clamps_the_top(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x4000], vec![0x22; 0x4000]);
    open_hex(
        cx,
        &rig,
        &[(0x3ff0..0x3ff4, 0x3ff0..0x3ff4)],
        HexAlign::Offset,
    );
    rig.press(cx, "f7");
    let page = visible(cx, &rig);
    let top = rig.view.update(cx, |view, _| view.file_hex_wheel(0., page));
    assert_eq!(
        top,
        Some(0x400 - page),
        "a step in the last page leaves the top on the last page"
    );
    let up = rig
        .view
        .update(cx, |view, _| view.file_hex_wheel(-1., page));
    assert_eq!(up, Some(0x400 - page - 1), "the first upward tick scrolls");
}

/// Wheels the hex body to row `top` with no frame or request after it.
fn wheel_to(cx: &mut TestAppContext, rig: &Rig, top: usize) {
    let page = visible(cx, rig);
    rig.view.update(cx, |view, _| {
        view.file_hex_wheel(-1e9, page);
        view.file_hex_wheel(top as f64, page);
    });
}

/// Scrolls the hex body to row `top` and asks for its rows without letting the read land.
fn scroll_and_request(cx: &mut TestAppContext, rig: &Rig, top: usize) {
    wheel_to(cx, rig, top);
    cx.update_window(rig.window, |_, window, cx| {
        rig.view
            .update(cx, |view, cx| view.request_hex_rows(window, cx));
    })
    .expect("window");
}

/// Reads whole pages from the top until the byte cache is past its limit; returns the next page.
fn fill_cache(cx: &mut TestAppContext, rig: &Rig) -> usize {
    let page = visible(cx, rig);
    let mut next = page;
    while probe(cx, rig).cached <= probe(cx, rig).cache_limit {
        scroll_and_request(cx, rig, next);
        settle(cx, &rig.view);
        next += page;
    }
    next
}

#[gpui_kit::test]
fn compare_screen_hex_eviction_keeps_the_visible_rows(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, patterned(0x10_0000, 0), patterned(0x10_0000, 0x55));
    open_hex(cx, &rig, &[], HexAlign::Offset);
    let far = fill_cache(cx, &rig);
    let page = visible(cx, &rig);
    scroll_and_request(cx, &rig, far + 5000);
    scroll_and_request(cx, &rig, 1);
    let on_screen =
        |cx: &mut TestAppContext| probe(cx, &rig).loaded.iter().take(page).all(|row| *row);
    assert!(on_screen(cx), "rows 1.. are cached before the read lands");
    for tick in 0..200 {
        assert!(
            on_screen(cx),
            "tick {tick}: the late read must not blank the rows on screen"
        );
        if !cx.executor().tick() {
            break;
        }
    }
    settle(cx, &rig.view);
    wheel_to(cx, &rig, far + 5000);
    assert!(
        probe(cx, &rig).loaded.iter().take(page).all(|row| *row),
        "the far read landed, so its rows are cached"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_eviction_keeps_the_viewport_at_the_current_top(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, patterned(0x10_0000, 0), patterned(0x10_0000, 0x55));
    open_hex(cx, &rig, &[], HexAlign::Offset);
    let far = fill_cache(cx, &rig);
    let page = visible(cx, &rig);
    scroll_and_request(cx, &rig, far + 5000);
    wheel_to(cx, &rig, far - page);
    let on_screen =
        |cx: &mut TestAppContext| probe(cx, &rig).loaded.iter().take(page).all(|row| *row);
    assert!(on_screen(cx), "the rows at the new top are cached");
    for tick in 0..200 {
        assert!(
            on_screen(cx),
            "tick {tick}: the read must not blank the rows at the new top"
        );
        if !cx.executor().tick() {
            break;
        }
    }
    settle(cx, &rig.view);
    wheel_to(cx, &rig, far + 5000);
    assert!(
        probe(cx, &rig).loaded.iter().take(page).all(|row| *row),
        "the far read landed"
    );
}

#[gpui_kit::test]
fn compare_screen_hex_cache_stays_bounded(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, patterned(0x10_0000, 0), patterned(0x10_0000, 0x55));
    open_hex(cx, &rig, &[], HexAlign::Offset);
    let page = visible(cx, &rig);
    let mut peak = 0;
    for step in 1..=(2 * 4096 / page + 4) {
        scroll_and_request(cx, &rig, step * page);
        settle(cx, &rig.view);
        let seen = probe(cx, &rig);
        peak = peak.max(seen.cached);
        assert!(
            seen.cached <= seen.cache_limit + 2 * page,
            "step {step}: {} spans cached",
            seen.cached
        );
        assert!(
            seen.loaded.iter().take(page).all(|row| *row),
            "step {step} is blank"
        );
    }
    assert!(peak > 4096, "the cache did fill past its limit: {peak}");
}

#[gpui_kit::test]
fn compare_screen_hex_details_drawer_lists_kind_sizes_alignment_and_regions(
    cx: &mut TestAppContext,
) {
    let _shown = crate::compare_visible::show_compare();
    let rig = hex_rig(cx, vec![0x11; 0x4000], vec![0x22; 0x4000]);
    open_hex(
        cx,
        &rig,
        &[(0x1000..0x1004, 0x1000..0x1004)],
        HexAlign::Offset,
    );
    rig.press(cx, "ctrl-i");
    settle(cx, &rig.view);
    let facts = rig.view.read_with(cx, |view, _| view.body_facts_probe());
    let want = [
        "Kind: Binary",
        "Sizes: 16.0 KB ↔ 16.0 KB",
        "Alignment: Align by offset",
        "Progress: 1 regions",
    ];
    assert_eq!(facts, Some(want.map(str::to_owned).to_vec()));
}
