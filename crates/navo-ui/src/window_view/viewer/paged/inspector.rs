//! The hex inspector drawer (Ctrl+I in hex mode): reads the bytes at the cursor off the UI
//! thread and shows what `inspect` decodes from them.

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{Stateful, Task};
use navo_core::paging::read_full;
use navo_l10n::{t, tr};

use super::PagedView;
use super::inspect::{INSPECT_BYTES, Inspection, inspect};
use super::inspect_scan::InspectorScan;
use super::lines::PagedKind;
use crate::prelude::*;
use crate::primitives::{IconButton, IconName};
use crate::side_sheet::{SheetFrame, SheetState, Side, SideSheet};
use crate::tokens::Theme;

const INSPECTOR_WIDTH: f32 = 340.;

/// The cursor offset and source revision a read belongs to.
type ReadKey = (u64, u64);

/// The bytes read for one cursor place, or why the read failed.
struct Read {
    key: ReadKey,
    bytes: Result<Vec<u8>, String>,
}

/// The drawer's open state, the last read and the read in flight.
pub(super) struct HexInspector {
    sheet: Entity<SheetState>,
    read: Option<Read>,
    job: Option<(ReadKey, Task<()>)>,
    pub(super) scan: InspectorScan,
}

impl HexInspector {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            sheet: cx.new(|_| SheetState::new()),
            read: None,
            job: None,
            scan: InspectorScan::new(cx),
        }
    }
}

fn value_row(t: &Theme, at: usize, label: String, value: String) -> ObservedElement<Stateful<Div>> {
    div()
        .id(("hex-inspector-row", at))
        .aria_label(format!("{label} {value}"))
        .test_support()
        .flex()
        .justify_between()
        .gap(t.spacing.md)
        .child(div().text_color(t.colors.muted).child(label))
        .child(
            div()
                .font_family(t.typography.mono_font.clone())
                .child(value),
        )
}

fn decode_rows(t: &Theme, inspection: Inspection) -> Stateful<Div> {
    div()
        .id("hex-inspector-body")
        .flex()
        .flex_col()
        .flex_none()
        .max_h(relative(0.45))
        .overflow_y_scroll()
        .gap(t.spacing.xs)
        .p(t.spacing.xl)
        .children(
            inspection
                .rows
                .into_iter()
                .enumerate()
                .map(|(at, row)| value_row(t, at, tr(row.id), row.value)),
        )
}

impl PagedView {
    pub(super) fn is_inspecting(&self, cx: &App) -> bool {
        self.kind == PagedKind::Hex && self.inspector.sheet.read(cx).is_open()
    }

    /// Ctrl+I opens or closes the inspector in hex mode; text mode leaves it to the viewer.
    pub(super) fn right_drawer(&mut self, cx: &mut Context<Self>) {
        if self.kind != PagedKind::Hex {
            cx.propagate();
            return;
        }
        let is_open = self.inspector.sheet.read(cx).is_open();
        self.inspector.sheet.update(cx, |sheet, cx| {
            if is_open {
                sheet.close(cx);
            } else {
                sheet.open(cx);
            }
        });
        cx.notify();
    }

    /// Esc closes the open inspector first; true when it did.
    pub(super) fn close_inspector(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.is_inspecting(cx) {
            return false;
        }
        self.inspector.sheet.update(cx, |sheet, cx| sheet.close(cx));
        cx.notify();
        true
    }

    /// Starts a background read of the bytes at the cursor when the drawer shows older ones.
    pub(super) fn request_inspection(&mut self, cx: &mut Context<Self>) {
        if !self.is_inspecting(cx) {
            return;
        }
        self.request_scan(cx);
        let key = (self.cursor, self.source_rev);
        let is_read = self
            .inspector
            .read
            .as_ref()
            .is_some_and(|read| read.key == key);
        let is_reading = self.inspector.job.as_ref().is_some_and(|job| job.0 == key);
        if is_read || is_reading {
            return;
        }
        let source = self.source.clone();
        let task = cx.spawn(async move |this, cx| {
            let bytes = cx
                .background_spawn(async move {
                    let mut buf = [0u8; INSPECT_BYTES];
                    read_full(&*source, key.0, &mut buf).map(|read| buf[..read].to_vec())
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.inspector.job = None;
                view.inspector.read = Some(Read { key, bytes });
                cx.notify();
            });
        });
        self.inspector.job = Some((key, task));
    }

    /// The header button that opens the inspector by mouse, shown in hex mode.
    pub(super) fn inspector_button(&self, cx: &mut Context<Self>) -> Option<IconButton> {
        (self.kind == PagedKind::Hex).then(|| {
            IconButton::new(
                "hex-inspector-toggle",
                IconName::Info,
                tr("hex-inspector-button"),
            )
            .selected(self.inspector.sheet.read(cx).is_open())
            .on_click(cx.listener(|view, _, _, cx| view.right_drawer(cx)))
        })
    }

    /// The decoded values above, the hashes and strings below.
    fn drawer_body(&self, t: &Theme, bytes: &[u8], cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(decode_rows(t, inspect(bytes)))
            .child(self.scan_section(t, cx))
    }

    /// The inspector drawer over the rows, while it is open or closing.
    pub(super) fn inspector_drawer(
        &self,
        t: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let sheet = &self.inspector.sheet;
        if self.kind != PagedKind::Hex || !sheet.read(cx).is_shown(cx) {
            return None;
        }
        let key = (self.cursor, self.source_rev);
        let read = self.inspector.read.as_ref();
        let status = match read {
            Some(Read {
                bytes: Err(error), ..
            }) => Some(error.clone()),
            Some(read) if read.key == key => None,
            _ => Some(tr("hex-inspector-reading")),
        }
        .or_else(|| self.inspector.scan.copied_note());
        let bytes = read
            .and_then(|read| read.bytes.as_deref().ok())
            .unwrap_or(&[]);
        let body = window.viewport_size().width - t.spacing.md * 2.;
        let frame = SheetFrame::new(body, px(INSPECTOR_WIDTH), Side::Left).covering_passive_card();
        let this = cx.weak_entity();
        let offset = format!("0x{:08X}", self.cursor);
        let mut drawer = SideSheet::new("hex-inspector", sheet, frame)
            .label(tr("hex-inspector-label"))
            .header(
                div()
                    .font_family(t.typography.brand_font.clone())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t!("hex-inspector-title", offset = offset)),
            )
            .close("ctrl-i", move |_, cx| {
                let _ = this.update(cx, |view, cx| view.right_drawer(cx));
            })
            .body(self.drawer_body(t, bytes, cx))
            .hint(&["←", "→", "↑", "↓"], tr("hex-inspector-move"))
            .hint(&["tab"], tr("hex-inspector-tab"))
            .hint(&["enter"], tr("hex-inspector-enter"))
            .hint(&["escape"], tr("hex-inspector-close"));
        if let Some(status) = status {
            drawer = drawer.status(status);
        }
        Some(
            div()
                .absolute()
                .top(t.spacing.xs)
                .bottom(t.spacing.xs)
                .left(t.spacing.md)
                .right(t.spacing.md)
                .child(drawer),
        )
    }
}
