use std::rc::Rc;

use chrono::{TimeZone, Utc};
use rusty_core::Location;
use rusty_core::retention::RetentionId;
use rusty_ui::browser::{RetainedKind, RetainedRow, RetainedState};
use rusty_ui::prelude::*;
use rusty_ui::primitives::ListScroll;
use rusty_ui::recently_deleted::{Model, Phase, RecentlyDeletedBody, SheetParts, sheet, summary};
use rusty_ui::side_sheet::{SheetFrame, SheetState, Side};
use rusty_ui::tokens;

use crate::gallery::Page;

const DESKTOP: f32 = 1280.;
const PHONE: f32 = 390.;
const HOUR: i64 = 3_600_000;
const CURSOR: usize = 2;

type Sample = (i64, &'static str, u64, i64, (RetainedKind, RetainedState));

const SAMPLES: [Sample; 7] = [
    (
        9,
        r"D:\Work\Traces\trace-2026-10-26.blf",
        520_000_000,
        HOUR / 3,
        (RetainedKind::Delete, RetainedState::Busy),
    ),
    (
        8,
        r"D:\Work\Reports\report-final-v3.xlsx",
        188_416,
        2 * HOUR,
        (RetainedKind::Delete, RetainedState::Ready),
    ),
    (
        7,
        r"D:\Work\Builds\TC397\linker\flash.ld",
        4096,
        5 * HOUR,
        (RetainedKind::Overwrite, RetainedState::Ready),
    ),
    (
        6,
        r"D:\Work\Releases\release-v0.7.zip",
        398_458_880,
        20 * HOUR,
        (RetainedKind::Delete, RetainedState::Ready),
    ),
    (
        5,
        r"C:\Users\admin\Documents\a-very-long-file-name-for-checking-how-the-row-truncates.md",
        12_288,
        22 * HOUR,
        (RetainedKind::Delete, RetainedState::NeedsAttention),
    ),
    (
        4,
        r"C:\Users\admin\Desktop\Untitled-1.csv",
        2048,
        66 * HOUR + HOUR / 2,
        (RetainedKind::Delete, RetainedState::Ready),
    ),
    (
        3,
        r"D:\Work\Repos\ecu-tools\vendor\notes.txt",
        900,
        68 * HOUR,
        (RetainedKind::Delete, RetainedState::Ready),
    ),
];

fn now_ms() -> i64 {
    Utc.with_ymd_and_hms(2026, 10, 28, 14, 0, 0)
        .single()
        .map_or(0, |moment| moment.timestamp_millis())
}

fn rows() -> Vec<RetainedRow> {
    let now = now_ms();
    SAMPLES
        .iter()
        .filter_map(|&(id, path, bytes, ago, (kind, state))| {
            let deleted_at_ms = now - ago;
            Some(RetainedRow {
                id: RetentionId(id),
                original: Location::new(path).ok()?,
                kind,
                bytes,
                deleted_at_ms,
                expires_at_ms: deleted_at_ms + 72 * HOUR,
                state,
            })
        })
        .collect()
}

pub struct RecentPage {
    sheet: Entity<SheetState>,
    scroll: ListScroll,
    model: Rc<Model>,
    width: f32,
}

impl RecentPage {
    pub fn new(width: f32, cx: &mut Context<Self>) -> Self {
        let sheet = cx.new(|_| SheetState::new());
        sheet.update(cx, |sheet, cx| sheet.open(cx));
        Self {
            sheet,
            scroll: ListScroll::new(),
            model: Rc::new(Model::new(&rows(), now_ms(), &Utc)),
            width,
        }
    }
}

impl Render for RecentPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let body = px(self.width) - t.spacing.xl;
        let frame = SheetFrame::new(body, body, Side::Left);
        let shown = RecentlyDeletedBody::new(&self.model, Phase::Ready, &self.scroll)
            .cursor(CURSOR)
            .now_ms(now_ms());
        let parts = SheetParts {
            subtitle: summary(self.model.items(), self.model.bytes()).into(),
            status: None,
            has_rows: true,
        };
        div()
            .relative()
            .flex_none()
            .w(px(self.width))
            .h_full()
            .bg(t.colors.background)
            .child(
                div()
                    .absolute()
                    .top(t.spacing.xs)
                    .bottom(t.spacing.xs)
                    .left(t.spacing.md)
                    .right(t.spacing.md)
                    .child(sheet(&self.sheet, frame, shown, parts)),
            )
    }
}

pub fn pages(cx: &mut App) -> [Entity<RecentPage>; 2] {
    [DESKTOP, PHONE].map(|width| cx.new(|cx| RecentPage::new(width, cx)))
}

pub fn element(page: Page, pages: &[Entity<RecentPage>; 2]) -> Option<AnyElement> {
    let shown = match page {
        Page::RecentlyDeleted => &pages[0],
        Page::RecentlyDeletedPhone => &pages[1],
        _ => return None,
    };
    Some(shown.clone().into_any_element())
}
