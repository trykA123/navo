use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, TestAppContext, Window, div,
};
use rusty_core::rename::check::{Existing, ExistingName, Fold, Problem, RowState};
use rusty_core::{Location, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use crate::multi_rename::{
    Form, Item, Listing, Preview, PreviewEvent, PreviewRow, PreviewView, Shown, Summary,
};
use crate::test_window::open;

const SCOPE: &str = "rename-preview";
const CHANGES: RowState = RowState::Changes { case_only: false };

struct Host {
    view: Entity<PreviewView>,
    events: Rc<RefCell<Vec<PreviewEvent>>>,
    _events: Subscription,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.clone())
    }
}

fn folder() -> Location {
    Location::new("/work/projects/photos").unwrap()
}

fn items(names: &[&str]) -> Vec<Item> {
    names
        .iter()
        .map(|name| {
            let entry = FsEntry {
                name: ProviderPath::from_text(name),
                kind: EntryKind::File,
                size: 0,
                modified_ms: None,
                attributes: Attributes::default(),
            };
            Item::from_entry(&folder(), &entry)
        })
        .collect()
}

fn crafted(rows: &[(&str, &str, RowState)]) -> (Vec<Item>, Preview) {
    let names: Vec<&str> = rows.iter().map(|(old, ..)| *old).collect();
    let rows: Vec<PreviewRow> = rows
        .iter()
        .map(|(_, new, state)| PreviewRow {
            new: (*new).to_owned(),
            state: *state,
        })
        .collect();
    let problems = rows
        .iter()
        .filter(|row| matches!(row.state, RowState::Problem(_)))
        .count();
    let summary = Summary {
        problems,
        ..Summary::default()
    };
    let preview = Preview {
        generation: 1,
        rows,
        summary,
        plan: None,
    };
    (items(&names), preview)
}

fn computed(mask: &str, names: &[&str], taken: &[&str]) -> (Vec<Item>, Preview) {
    let items = items(names);
    let mut form = Form::new();
    form.set_name_mask(mask);
    let existing = Existing::new(
        taken
            .iter()
            .map(|name| ExistingName {
                parent: folder(),
                name: (*name).to_owned(),
            })
            .collect(),
    );
    let listing = Listing {
        existing: &existing,
        fold: Fold::Exact,
    };
    let preview = Preview::compute(&form.compile().unwrap(), &items, listing, 1);
    (items, preview)
}

fn open_host(
    cx: &mut TestAppContext,
    (items, preview): (Vec<Item>, Preview),
    problems_only: bool,
) -> (AnyWindowHandle, Entity<Host>) {
    let (window, host) = open(cx, move |window, cx| {
        let view = cx.new(|cx| PreviewView::new(window, cx));
        view.update(cx, |view, cx| {
            view.set_preview(
                Shown {
                    items: &items,
                    preview: &preview,
                    fold: Fold::Exact,
                    problems_only,
                },
                cx,
            );
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        let subscription = cx.subscribe(&view, move |_, event: &PreviewEvent, _| {
            seen.borrow_mut().push(*event);
        });
        Host {
            view,
            events,
            _events: subscription,
        }
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    cx.run_until_parked();
    (window, host)
}

fn row_label(window: AnyWindowHandle, at: usize, cx: &mut TestAppContext) -> Option<String> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(SCOPE)
            .try_find(("row", at))
            .and_then(|row| row.label().map(str::to_owned))
    })
    .expect("window")
}

fn click(window: AnyWindowHandle, id: impl Into<gpui_kit::ElementId>, cx: &mut TestAppContext) {
    let id = id.into();
    cx.update_window(window, |_, window, cx| window.within(SCOPE).click(id, cx))
        .expect("window");
    cx.run_until_parked();
}

fn events(host: &Entity<Host>, cx: &mut TestAppContext) -> Vec<PreviewEvent> {
    host.read_with(cx, |host, _| host.events.borrow().clone())
}

#[gpui_kit::test]
fn swaps_case_only_and_plain_renames_are_told_apart(cx: &mut TestAppContext) {
    let rows = [
        ("a.txt", "b.txt", CHANGES),
        ("b.txt", "a.txt", CHANGES),
        ("c.txt", "d.txt", CHANGES),
        ("E.txt", "e.txt", RowState::Changes { case_only: true }),
    ];
    let (window, _) = open_host(cx, crafted(&rows), false);
    let label = |at, cx: &mut TestAppContext| row_label(window, at, cx).unwrap();
    assert_eq!(label(0, cx), "a.txt, b.txt, Swap");
    assert_eq!(label(1, cx), "b.txt, a.txt, Swap");
    assert_eq!(label(2, cx), "c.txt, d.txt, ");
    assert_eq!(label(3, cx), "E.txt, e.txt, Case only");
}

#[gpui_kit::test]
fn a_problem_row_gives_its_reason_and_the_other_row_number(cx: &mut TestAppContext) {
    let rows = [
        ("a.txt", "x.txt", CHANGES),
        ("b.txt", "x.txt", RowState::Problem(Problem::Duplicate(0))),
        ("c.txt", "taken.txt", RowState::Problem(Problem::Exists)),
        ("d.txt", "", RowState::Problem(Problem::Empty)),
    ];
    let (window, _) = open_host(cx, crafted(&rows), false);
    let label = |at, cx: &mut TestAppContext| row_label(window, at, cx).unwrap();
    assert!(label(1, cx).ends_with("Same new name as row 1"));
    assert!(label(2, cx).ends_with("Already exists"));
    assert!(label(3, cx).ends_with("Name is empty"));
}

#[gpui_kit::test]
fn problems_only_lists_just_the_problem_rows(cx: &mut TestAppContext) {
    let rows = [
        ("a.txt", "x.txt", CHANGES),
        ("b.txt", "x.txt", RowState::Problem(Problem::Duplicate(0))),
        ("c.txt", "c.txt", RowState::Unchanged),
    ];
    let (window, _) = open_host(cx, crafted(&rows), true);
    assert!(row_label(window, 0, cx).unwrap().starts_with("b.txt"));
    assert_eq!(row_label(window, 1, cx), None);
}

#[gpui_kit::test]
fn the_toggle_reports_its_row_and_nothing_else(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, computed("x_[N]", &["a.txt", "b.txt"], &[]), false);
    click(window, ("rename-toggle", 1usize), cx);
    assert_eq!(events(&host, cx), vec![PreviewEvent::RowToggled(1)]);
    host.read_with(cx, |host, cx| {
        assert_eq!(host.view.read(cx).cursor(), None);
    });
}

#[gpui_kit::test]
fn rename_runs_when_there_is_no_problem(cx: &mut TestAppContext) {
    let (window, host) = open_host(cx, computed("x_[N]", &["a.txt", "b.txt"], &[]), false);
    click(window, "rename-run", cx);
    click(window, "rename-cancel", cx);
    assert_eq!(
        events(&host, cx),
        vec![PreviewEvent::Run, PreviewEvent::Cancel]
    );
}

#[gpui_kit::test]
fn rename_does_nothing_while_a_problem_exists(cx: &mut TestAppContext) {
    let batch = computed("x_[N]", &["a.txt", "b.txt"], &["x_b.txt"]);
    assert!(batch.1.summary.problems > 0);
    let (window, host) = open_host(cx, batch, false);
    click(window, "rename-run", cx);
    assert!(events(&host, cx).is_empty());
}

#[gpui_kit::test]
fn fifty_thousand_rows_draw_only_the_visible_ones(cx: &mut TestAppContext) {
    let names: Vec<String> = (0..50_000).map(|at| format!("IMG_{at:05}.jpg")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let (window, _) = open_host(cx, computed("x_[N]", &names, &[]), false);
    assert!(row_label(window, 0, cx).is_some());
    assert!(row_label(window, 40, cx).is_none());
}
