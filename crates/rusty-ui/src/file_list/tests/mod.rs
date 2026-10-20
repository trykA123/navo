mod density;
mod input;
mod marks;
mod parts;
mod render;

use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, FocusHandle, IntoElement, ParentElement,
    Pixels, Render, Styled, TestAppContext, Window, div, px,
};
use rusty_core::{Command, ProviderPath, Selection, ViewState};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use super::{FileList, FileListAction, FileListProps};
use crate::test_window::open;

pub(super) const MODIFIED_MS: i64 = 1_791_920_400_000;

pub(super) fn entry(name: &str, kind: EntryKind, size: u64) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind,
        size,
        modified_ms: Some(MODIFIED_MS),
        attributes: Attributes::default(),
    }
}

pub(super) fn sample_rows() -> Rc<[FsEntry]> {
    let mut hidden = entry(".env", EntryKind::File, 512);
    hidden.attributes.hidden = true;
    vec![
        entry("src", EntryKind::Folder, 0),
        entry("tests", EntryKind::Folder, 0),
        entry("Cargo.toml", EntryKind::File, 812),
        entry("layout.rs", EntryKind::File, 14_336),
        entry("docs", EntryKind::Link, 0),
        hidden,
    ]
    .into()
}

pub(super) fn generated_rows(count: usize) -> Rc<[FsEntry]> {
    (0..count)
        .map(|index| entry(&format!("bulk_{index:06}.jpg"), EntryKind::File, 4_096))
        .collect()
}

pub(super) fn props(rows: Rc<[FsEntry]>) -> FileListProps {
    let len = rows.len() as u64;
    FileListProps {
        folder: "core".into(),
        rows,
        view: ViewState::default(),
        marked: Rc::new(Selection::new(len)),
        is_active: true,
        filter: None,
        sort: crate::file_list::SortKey::default(),
    }
}

pub(super) struct Harness {
    pub props: FileListProps,
    pub focus: FocusHandle,
    pub height: Pixels,
    pub commands: Vec<Command>,
    pub actions: Vec<FileListAction>,
    pub page_rows: Vec<u64>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = FileList::new("left", self.props.clone())
            .track_focus(&self.focus)
            .on_command(cx.listener(|this, command: &Command, _, _| {
                this.commands.push(command.clone());
            }))
            .on_list_action(cx.listener(|this, action: &FileListAction, _, _| {
                this.actions.push(*action);
            }))
            .on_page_rows(cx.listener(|this, rows: &u64, _, _| this.page_rows.push(*rows)));
        div()
            .size_full()
            .child(div().h(self.height).flex().flex_col().child(list))
    }
}

pub(super) fn open_list(
    cx: &mut TestAppContext,
    props: FileListProps,
) -> (AnyWindowHandle, Entity<Harness>) {
    let (window, harness) = open(cx, |_, cx| Harness {
        props,
        focus: cx.focus_handle(),
        height: px(480.),
        commands: Vec::new(),
        actions: Vec::new(),
        page_rows: Vec::new(),
    });
    cx.update_window(window, |_, window, cx| {
        let focus = harness.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.render_frame(cx);
    })
    .expect("window");
    (window, harness)
}

pub(super) fn commands(harness: &Entity<Harness>, cx: &mut TestAppContext) -> Vec<Command> {
    harness.read_with(cx, |harness, _| harness.commands.clone())
}

pub(super) fn actions(harness: &Entity<Harness>, cx: &mut TestAppContext) -> Vec<FileListAction> {
    harness.read_with(cx, |harness, _| harness.actions.clone())
}
