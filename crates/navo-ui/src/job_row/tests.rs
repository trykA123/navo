use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, ElementId, Entity, IntoElement, ParentElement,
    Render, Role, Styled, TestAppContext, Window, div,
};

use super::{JobRow, JobRowProps, JobRowState};

const ROW: &str = "job";
const GB: u64 = 1024 * 1024 * 1024;

fn running() -> JobRowProps {
    JobRowProps {
        title: r"Copying 12 files to D:\Backup".into(),
        current: Some(r"D:\work\photos\IMG_4120.jpg".into()),
        bytes_done: GB * 12 / 10,
        bytes_total: GB * 34 / 10,
        files_done: 4,
        files_total: 12,
        state: JobRowState::Running,
    }
}

fn with_state(state: JobRowState) -> JobRowProps {
    JobRowProps { state, ..running() }
}

struct Harness {
    props: JobRowProps,
    cancels: usize,
    dismissals: usize,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (this, that) = (cx.entity(), cx.entity());
        let row = JobRow::new(ROW, self.props.clone())
            .on_cancel(move |_, cx| this.update(cx, |this, _| this.cancels += 1))
            .on_dismiss(move |_, cx| that.update(cx, |that, _| that.dismissals += 1));
        div().w_full().child(row)
    }
}

fn open_row(cx: &mut TestAppContext, props: JobRowProps) -> (AnyWindowHandle, Entity<Harness>) {
    let (window, harness) = crate::test_window::open(cx, |_, _| Harness {
        props,
        cancels: 0,
        dismissals: 0,
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    (window, harness)
}

fn label(
    window: AnyWindowHandle,
    id: impl Into<ElementId>,
    cx: &mut TestAppContext,
) -> Option<String> {
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(ROW)
            .try_find(id)
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

fn has(window: AnyWindowHandle, id: &'static str, cx: &mut TestAppContext) -> bool {
    cx.update_window(window, |_, window, _| {
        window.within(ROW).try_find(id).is_some()
    })
    .expect("window")
}

#[test]
fn the_fraction_follows_the_bytes() {
    let mut props = running();
    props.bytes_done = 250;
    props.bytes_total = 1_000;
    assert!((props.fraction() - 0.25).abs() < f32::EPSILON);
    props.bytes_done = 1_000;
    assert!((props.fraction() - 1.).abs() < f32::EPSILON);
    props.bytes_done = 2_000;
    assert!((props.fraction() - 1.).abs() < f32::EPSILON);
}

#[test]
fn a_zero_total_gives_zero_percent() {
    let mut props = running();
    props.bytes_done = 0;
    props.bytes_total = 0;
    assert!(props.fraction().abs() < f32::EPSILON);
    assert_eq!(props.percent_text(), "0%");
}

#[test]
fn the_progress_text_reads_sizes_and_files() {
    assert_eq!(
        running().progress_text(),
        "1.2 GB of 3.4 GB · 4 of 12 files"
    );
    assert_eq!(running().percent_text(), "35%");
}

#[gpui_kit::test]
fn a_running_row_shows_its_title_file_bar_and_cancel(cx: &mut TestAppContext) {
    let (window, harness) = open_row(cx, running());
    assert_eq!(
        label(window, "title", cx).as_deref(),
        Some(r"Copying 12 files to D:\Backup")
    );
    assert_eq!(
        label(window, "current", cx).as_deref(),
        Some(r"D:\work\photos\IMG_4120.jpg")
    );
    assert_eq!(label(window, "bar", cx).as_deref(), Some("35%"));
    assert_eq!(
        label(window, "progress", cx).as_deref(),
        Some("1.2 GB of 3.4 GB · 4 of 12 files")
    );
    cx.update_window(window, |_, window, cx| {
        assert_eq!(
            window.within(ROW).find("bar").role(),
            Some(Role::ProgressIndicator)
        );
        window.within(ROW).click("cancel", cx);
    })
    .expect("window");
    assert_eq!(harness.read_with(cx, |harness, _| harness.cancels), 1);
}

#[gpui_kit::test]
fn finished_rows_have_no_cancel(cx: &mut TestAppContext) {
    for state in [
        JobRowState::Done,
        JobRowState::Failed("Access denied".into()),
        JobRowState::Cancelled,
    ] {
        let (window, _) = open_row(cx, with_state(state.clone()));
        assert!(!has(window, "cancel", cx), "{state:?}");
    }
}

#[gpui_kit::test]
fn finished_rows_name_their_state(cx: &mut TestAppContext) {
    let cases = [
        (JobRowState::Done, "Done"),
        (JobRowState::Failed("Access denied".into()), "Failed"),
        (JobRowState::Cancelled, "Cancelled"),
    ];
    for (state, text) in cases {
        let (window, _) = open_row(cx, with_state(state));
        assert_eq!(label(window, "status", cx).as_deref(), Some(text));
    }
}

#[gpui_kit::test]
fn a_failed_row_shows_its_message_instead_of_the_file(cx: &mut TestAppContext) {
    let (window, _) = open_row(cx, with_state(JobRowState::Failed("Access denied".into())));
    assert_eq!(
        label(window, "message", cx).as_deref(),
        Some("Access denied")
    );
    assert_eq!(label(window, "current", cx), None);
}

#[gpui_kit::test]
fn the_row_reads_as_one_list_item(cx: &mut TestAppContext) {
    let (window, _) = open_row(cx, running());
    cx.update_window(window, |_, window, _| {
        let row = window.find(ROW);
        assert_eq!(row.role(), Some(Role::ListItem));
        assert_eq!(
            row.label(),
            Some(r"Copying 12 files to D:\Backup, 35%, 1.2 GB of 3.4 GB · 4 of 12 files")
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_failed_row_can_be_dismissed(cx: &mut TestAppContext) {
    let (window, harness) = open_row(cx, with_state(JobRowState::Failed("Access denied".into())));
    cx.update_window(window, |_, window, cx| {
        window.within(ROW).click("dismiss", cx);
    })
    .expect("window");
    assert_eq!(harness.read_with(cx, |harness, _| harness.dismissals), 1);
}

#[gpui_kit::test]
fn only_a_failed_row_offers_dismiss(cx: &mut TestAppContext) {
    for state in [
        JobRowState::Running,
        JobRowState::Done,
        JobRowState::Cancelled,
    ] {
        let (window, _) = open_row(cx, with_state(state.clone()));
        assert!(!has(window, "dismiss", cx), "{state:?}");
    }
}
