use rusty_core::rename::check::{Existing, Fold, Problem, RowState};
use rusty_core::{Location, NameError, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};
use rusty_ui::multi_rename::{
    Form, Item, Listing, Preview, PreviewRow, PreviewView, Shown, Summary,
};
use rusty_ui::prelude::*;
use rusty_ui::tokens;

const LARGE: usize = 50_000;
const LARGE_CURSOR: usize = 24_000;
const CHANGES: RowState = RowState::Changes { case_only: false };

const STATES: [(&str, &str, RowState); 15] = [
    ("IMG_0001.jpg", "holiday_001.jpg", CHANGES),
    ("IMG_0002.jpg", "holiday_002.jpg", CHANGES),
    (
        "notes.TXT",
        "notes.txt",
        RowState::Changes { case_only: true },
    ),
    ("a.txt", "b.txt", CHANGES),
    ("b.txt", "a.txt", CHANGES),
    ("readme.md", "readme.md", RowState::Unchanged),
    ("draft.doc", "draft.doc", RowState::Excluded),
    (
        "report.pdf",
        "holiday_001.jpg",
        RowState::Problem(Problem::Duplicate(0)),
    ),
    (
        "scan.png",
        "scan:1.png",
        RowState::Problem(Problem::Invalid(NameError::Character(':'))),
    ),
    ("old.zip", "", RowState::Problem(Problem::Empty)),
    (
        "photo.jpg",
        "archive.zip",
        RowState::Problem(Problem::Exists),
    ),
    (
        "backup.tar",
        "con",
        RowState::Problem(Problem::Invalid(NameError::Reserved)),
    ),
    (
        "Screenshot 2026-10-20 at 14.32.07 - conference slides final version.png",
        "2026-10-20_14-32-07_conference_slides_final_version.png",
        CHANGES,
    ),
    ("budget.xlsx", "budget_2026.xlsx", CHANGES),
    ("todo.txt", "todo.txt", RowState::Unchanged),
];

fn item(name: &str) -> Option<Item> {
    let folder = Location::new("/work/projects/photos").ok()?;
    let entry = FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 0,
        modified_ms: None,
        attributes: Attributes::default(),
    };
    Some(Item::from_entry(&folder, &entry))
}

fn states() -> Option<(Vec<Item>, Preview)> {
    let items: Vec<Item> = STATES
        .iter()
        .map(|(old, ..)| item(old))
        .collect::<Option<_>>()?;
    let rows: Vec<PreviewRow> = STATES
        .iter()
        .map(|(_, new, state)| PreviewRow {
            new: (*new).to_owned(),
            state: *state,
        })
        .collect();
    let count =
        |wanted: fn(&RowState) -> bool| rows.iter().filter(|row| wanted(&row.state)).count();
    let summary = Summary {
        rename: count(|state| matches!(state, RowState::Changes { .. })),
        unchanged: count(|state| matches!(state, RowState::Unchanged)),
        excluded: count(|state| matches!(state, RowState::Excluded)),
        problems: count(|state| matches!(state, RowState::Problem(_))),
    };
    let preview = Preview {
        generation: 1,
        rows,
        summary,
        plan: None,
    };
    Some((items, preview))
}

fn large() -> Option<(Vec<Item>, Preview)> {
    let items: Vec<Item> = (0..LARGE)
        .map(|index| item(&format!("IMG_{index:05}.jpg")))
        .collect::<Option<_>>()?;
    let mut form = Form::new();
    form.set_name_mask("holiday_[C]");
    form.rules.counter.digits = 5;
    let compiled = form.compile().ok()?;
    let existing = Existing::default();
    let listing = Listing {
        existing: &existing,
        fold: Fold::Exact,
    };
    let preview = Preview::compute(&compiled, &items, listing, 1);
    Some((items, preview))
}

pub struct MultiRenamePreviewPage {
    view: Entity<PreviewView>,
}

impl MultiRenamePreviewPage {
    fn new(is_large: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let view = cx.new(|cx| PreviewView::new(window, cx));
        let sample = if is_large { large() } else { states() };
        let Some((items, preview)) = sample else {
            return Self { view };
        };
        view.update(cx, |view, cx| {
            view.set_preview(
                Shown {
                    items: &items,
                    preview: &preview,
                    fold: Fold::Exact,
                    problems_only: false,
                },
                cx,
            );
            if is_large {
                view.set_cursor(Some(LARGE_CURSOR), cx);
            }
        });
        Self { view }
    }
}

impl Render for MultiRenamePreviewPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .p(t.spacing.xl)
            .bg(t.colors.background)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .rounded(t.radii.md)
                    .border_1()
                    .border_color(t.colors.line)
                    .overflow_hidden()
                    .child(self.view.clone()),
            )
    }
}

pub type PreviewPages = [Entity<MultiRenamePreviewPage>; 2];

pub fn preview_pages(window: &mut Window, cx: &mut App) -> PreviewPages {
    [false, true].map(|is_large| cx.new(|cx| MultiRenamePreviewPage::new(is_large, window, cx)))
}
