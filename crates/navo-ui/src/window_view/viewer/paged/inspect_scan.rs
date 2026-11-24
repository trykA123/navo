//! The hashes and strings part of the hex inspector: runs `scan` off the UI thread, shows the
//! digests with copy buttons and a strings list whose rows move the byte cursor.

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Action, ClipboardItem, FocusHandle, Role, Stateful, Task, WeakEntity};
use navo_l10n::{t, tr};

use super::PagedView;
use super::scan::{MAX_STRINGS, SCAN_CHUNK, Scanned, scan};
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconButton, IconName, ListScroll, VirtualList};
use crate::tokens::{self, Theme};
use crate::window_view::viewer::search::FIND_ALL_CONTEXT;
use crate::window_view::viewer::{ListClose, ListDown, ListGo, ListUp, RightDrawer};

/// The hash rows above the strings, in list order; the selection counts them first.
const HASHES: [&str; 3] = [
    "hex-inspector-crc32",
    "hex-inspector-sha256",
    "hex-inspector-blake3",
];

/// A scan in flight; dropping it (new file revision, viewer closed) stops the read loop.
struct ScanJob {
    cancel: Arc<AtomicBool>,
    _task: Task<()>,
}

impl Drop for ScanJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The last scan, the one in flight and the list's selection and focus.
pub(super) struct InspectorScan {
    rev: Option<u64>,
    result: Option<Result<Arc<Scanned>, String>>,
    job: Option<ScanJob>,
    selected: usize,
    copied: Option<usize>,
    scroll: ListScroll,
    focus: FocusHandle,
}

impl InspectorScan {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            rev: None,
            result: None,
            job: None,
            selected: 0,
            copied: None,
            scroll: ListScroll::new(),
            focus: cx.focus_handle(),
        }
    }

    fn scanned(&self) -> Option<&Arc<Scanned>> {
        self.result.as_ref().and_then(|result| result.as_ref().ok())
    }

    fn digest(&self, row: usize) -> Option<String> {
        let digests = &self.scanned()?.digests;
        [&digests.crc32, &digests.sha256, &digests.blake3]
            .get(row)
            .map(|value| (*value).clone())
    }

    fn row_count(&self) -> usize {
        HASHES.len() + self.scanned().map_or(0, |scanned| scanned.strings.len())
    }

    /// The drawer status after a copy: "Copied SHA-256".
    pub(super) fn copied_note(&self) -> Option<String> {
        let name = tr(HASHES.get(self.copied?)?);
        Some(t!("hex-inspector-copied", name = name))
    }
}

impl PagedView {
    /// Starts a whole-file scan when the drawer has none for this file revision.
    pub(super) fn request_scan(&mut self, cx: &mut Context<Self>) {
        let scan_state = &mut self.inspector.scan;
        if scan_state.rev == Some(self.source_rev) {
            return;
        }
        scan_state.rev = Some(self.source_rev);
        scan_state.result = None;
        scan_state.copied = None;
        scan_state.selected = 0;
        let cancel = Arc::new(AtomicBool::new(false));
        let (source, flag) = (self.source.clone(), cancel.clone());
        let rev = self.source_rev;
        let task = cx.spawn(async move |this, cx| {
            let scanned = cx
                .background_spawn(async move { scan(&*source, SCAN_CHUNK, MAX_STRINGS, &flag) })
                .await;
            let _ = this.update(cx, |view, cx| view.land_scan(rev, scanned, cx));
        });
        scan_state.job = Some(ScanJob {
            cancel,
            _task: task,
        });
    }

    /// Keeps a finished scan when it still belongs to the shown file revision.
    fn land_scan(
        &mut self,
        rev: u64,
        scanned: Result<Option<Scanned>, String>,
        cx: &mut Context<Self>,
    ) {
        let scan_state = &mut self.inspector.scan;
        if scan_state.rev != Some(rev) {
            return;
        }
        scan_state.job = None;
        match scanned {
            Ok(Some(scanned)) => scan_state.result = Some(Ok(Arc::new(scanned))),
            Ok(None) => scan_state.rev = None,
            Err(error) => scan_state.result = Some(Err(error)),
        }
        cx.notify();
    }

    /// The kit's Tab and Shift+Tab actions, so the drawer can claim them while it is open.
    pub(super) fn inspector_tab_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let actions: Vec<_> = ["root::Tab", "root::TabPrev"]
            .into_iter()
            .filter_map(|name| cx.build_action(name, None).ok())
            .collect();
        actions.into_iter().fold(root, |root, action| {
            root.on_boxed_action(&*action, tab_listener(cx.weak_entity()))
        })
    }

    /// Tab, while the drawer is open in hex mode, moves the keyboard between the bytes and
    /// the hashes and strings list; otherwise the window's own Tab order runs.
    fn inspector_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_inspecting(cx) {
            cx.propagate();
            return;
        }
        if self.inspector.scan.focus.is_focused(window) {
            window.focus(&self.focus, cx);
        } else {
            window.focus(&self.inspector.scan.focus, cx);
        }
        cx.notify();
    }

    fn move_scan_row(&mut self, down: bool, cx: &mut Context<Self>) {
        let scan_state = &mut self.inspector.scan;
        let last = scan_state.row_count().saturating_sub(1);
        scan_state.selected = if down {
            (scan_state.selected + 1).min(last)
        } else {
            scan_state.selected.saturating_sub(1)
        };
        if let Some(string) = scan_state.selected.checked_sub(HASHES.len()) {
            scan_state.scroll.scroll_to(string);
        }
        cx.notify();
    }

    /// Copies hash `row` to the clipboard; nothing while it is still computing.
    fn copy_hash(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(value) = self.inspector.scan.digest(row) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        self.inspector.scan.copied = Some(row);
        cx.notify();
    }

    /// Moves the byte cursor to the start of string `index` and selects its row.
    fn go_to_string(&mut self, index: usize, cx: &mut Context<Self>) {
        let offset = self
            .inspector
            .scan
            .scanned()
            .and_then(|scanned| scanned.strings.get(index))
            .map(|found| found.offset);
        if let Some(offset) = offset {
            self.inspector.scan.selected = HASHES.len() + index;
            self.go_to_byte(offset, cx);
        }
    }

    /// Enter: copies the selected hash or goes to the selected string.
    fn act_on_scan_row(&mut self, cx: &mut Context<Self>) {
        let row = self.inspector.scan.selected;
        match row.checked_sub(HASHES.len()) {
            None => self.copy_hash(row, cx),
            Some(index) => self.go_to_string(index, cx),
        }
    }

    /// Esc or Ctrl+I in the list closes the drawer and gives the keyboard back to the bytes.
    fn leave_scan_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_inspector(cx);
        window.focus(&self.focus, cx);
    }

    fn hash_row(&self, t: &Theme, row: usize, cx: &mut Context<Self>) -> AnyElement {
        let scan_state = &self.inspector.scan;
        let is_selected = scan_state.selected == row;
        let value = match &scan_state.result {
            Some(Ok(_)) => scan_state.digest(row).unwrap_or_default(),
            Some(Err(_)) => "—".to_owned(),
            None => tr("hex-inspector-computing"),
        };
        let name = tr(HASHES[row]);
        let theme = t.selected_content(is_selected);
        let t = &theme;
        div()
            .when(t.is_contrast, |line| line.text_color(t.colors.text))
            .id(("hex-hash", row))
            .aria_label(format!("{name} {value}"))
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.md)
            .px(t.spacing.xl)
            .rounded(t.radii.md)
            .when(is_selected, |line| line.bg(t.colors.selection))
            .on_click(cx.listener(move |view, _, _, cx| {
                view.inspector.scan.selected = row;
                cx.notify();
            }))
            .child(
                div()
                    .flex_none()
                    .w(px(64.))
                    .text_color(t.colors.muted)
                    .child(name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(t.typography.mono_font.clone())
                    .child(value),
            )
            .child(self.copy_button(row, name, cx))
            .into_any_element()
    }

    /// The copy button of hash `row`, disabled while the hash is computing.
    fn copy_button(&self, row: usize, name: String, cx: &mut Context<Self>) -> impl IntoElement {
        div().id(("hex-hash-copy", row)).test_support().child(
            IconButton::new(
                ("hex-hash-copy-button", row),
                IconName::Copy,
                t!("hex-inspector-copy", name = name),
            )
            .disabled(self.inspector.scan.digest(row).is_none())
            .on_click(cx.listener(move |view, _, _, cx| view.copy_hash(row, cx))),
        )
    }

    fn strings_title(&self) -> String {
        match &self.inspector.scan.result {
            None => tr("hex-inspector-strings-computing"),
            Some(Err(error)) => error.clone(),
            Some(Ok(scanned)) => {
                let count = format::bytes(scanned.strings.len() as u64);
                if scanned.capped {
                    t!("hex-inspector-strings-capped", count = count)
                } else {
                    t!("hex-inspector-strings", count = count)
                }
            }
        }
    }

    /// The hashes, the strings title and the strings list, as one keyboard stop (Tab).
    pub(super) fn scan_section(&self, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let len = self
            .inspector
            .scan
            .scanned()
            .map_or(0, |scanned| scanned.strings.len());
        let this = cx.weak_entity();
        let list = VirtualList::new(
            "hex-strings",
            len,
            &self.inspector.scan.scroll,
            move |range, _, cx| string_rows(&this, range, cx),
        )
        .label(tr("hex-inspector-strings-label"));
        let title = self.strings_title();
        div()
            .id("hex-inspector-scan")
            .key_context(FIND_ALL_CONTEXT)
            .track_focus(&self.inspector.scan.focus)
            .on_action(cx.listener(|view, _: &ListUp, _, cx| view.move_scan_row(false, cx)))
            .on_action(cx.listener(|view, _: &ListDown, _, cx| view.move_scan_row(true, cx)))
            .on_action(cx.listener(|view, _: &ListGo, _, cx| view.act_on_scan_row(cx)))
            .on_action(
                cx.listener(|view, _: &ListClose, window, cx| view.leave_scan_list(window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &RightDrawer, window, cx| view.leave_scan_list(window, cx)),
            )
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(t.spacing.xs)
            .pt(t.spacing.md)
            .border_t_1()
            .border_color(t.colors.line)
            .children((0..HASHES.len()).map(|row| self.hash_row(t, row, cx)))
            .child(
                div()
                    .id("hex-strings-title")
                    .role(Role::Status)
                    .aria_label(title.clone())
                    .test_support()
                    .px(t.spacing.xl)
                    .pt(t.spacing.md)
                    .text_color(t.colors.muted)
                    .child(title),
            )
            .child(list)
    }
}

/// Runs the drawer's Tab handling on the view behind `view`.
fn tab_listener(view: WeakEntity<PagedView>) -> impl Fn(&dyn Action, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| view.inspector_tab(window, cx));
    }
}

/// Draws the visible strings rows; a click goes to the string.
fn string_rows(this: &WeakEntity<PagedView>, range: Range<usize>, cx: &mut App) -> Vec<AnyElement> {
    let Some(view) = this.upgrade() else {
        return Vec::new();
    };
    let t = tokens::current(cx).clone();
    let scan_state = &view.read(cx).inspector.scan;
    let Some(scanned) = scan_state.scanned().cloned() else {
        return Vec::new();
    };
    let selected = scan_state.selected.checked_sub(HASHES.len());
    let strings = scanned.strings.get(range.clone()).unwrap_or(&[]);
    range
        .zip(strings)
        .map(|(index, found)| {
            let theme = t.selected_content(selected == Some(index));
            let t = &theme;
            let offset = format!("0x{:08X}", found.offset);
            let this = this.clone();
            div()
                .when(t.is_contrast, |line| line.text_color(t.colors.text))
                .id(("hex-string-row", index))
                .role(Role::ListItem)
                .aria_label(format!("{offset} {}", found.text))
                .test_support()
                .flex()
                .gap(t.spacing.md)
                .w_full()
                .px(t.spacing.xl)
                .rounded(t.radii.md)
                .cursor_pointer()
                .font_family(t.typography.mono_font.clone())
                .when(selected == Some(index), |line| line.bg(t.colors.selection))
                .on_click(move |_, _, cx| {
                    let _ = this.update(cx, |view, cx| view.go_to_string(index, cx));
                })
                .child(div().flex_none().text_color(t.colors.muted).child(offset))
                .child(div().min_w_0().truncate().child(found.text.clone()))
                .into_any_element()
        })
        .collect()
}
