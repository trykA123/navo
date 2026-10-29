use std::sync::Arc;

use rusty_core::{Selection, ViewState};
use rusty_fs::FsEntry;
use rusty_ui::FocusHandle;
use rusty_ui::context_menu::{ContextMenu, ContextMenuInput};
use rusty_ui::file_list::{FileList, FileListProps, SortKey};
use rusty_ui::prelude::*;
use rusty_ui::tokens;

use rusty_core::shell_menu::Submenu;

use crate::menu_sample::{self, NEW, OPEN_WITH, SEND_TO, SEVEN_ZIP};
use crate::pane_sample;

const ZIP_ROW: u64 = 11;
const MARKED: [u64; 3] = [6, 7, 9];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shows {
    Root,
    SevenZip,
    Filter,
    Files,
    Background,
    EmptyFolder,
    Drop,
    Loading,
    Failed,
}

const FOLDER_ROWS: usize = 6;

pub struct MenuPage {
    shows: Shows,
    rows: Arc<[FsEntry]>,
    focus: FocusHandle,
    menu: Option<Entity<ContextMenu>>,
}

impl MenuPage {
    pub fn new(shows: Shows, cx: &mut Context<Self>) -> Self {
        let all = pane_sample::rows();
        let rows: Arc<[FsEntry]> = match shows {
            Shows::Background => all.iter().take(FOLDER_ROWS).cloned().collect(),
            Shows::EmptyFolder => Arc::from([]),
            _ => all,
        };
        Self {
            shows,
            rows,
            focus: cx.focus_handle(),
            menu: None,
        }
    }

    fn list(&self) -> FileList {
        let mut marked = Selection::new(self.rows.len() as u64);
        let cursor = match self.shows {
            Shows::Files => {
                marked.mark_rows(MARKED);
                MARKED[2]
            }
            Shows::Background | Shows::EmptyFolder => 0,
            _ => ZIP_ROW,
        };
        let props = FileListProps {
            folder: "commander".into(),
            rows: self.rows.clone(),
            view: ViewState { cursor, scroll: 0 },
            marked: Arc::new(marked),
            is_active: true,
            filter: None,
            sort: SortKey::default(),
        };
        FileList::new("menu-page-list", props).track_focus(&self.focus)
    }

    fn menu_input(&self, window: &Window, cx: &App) -> ContextMenuInput {
        let color = tokens::current(cx).colors.kind_archive;
        let (menu, context, anchor) = match self.shows {
            Shows::Drop => (
                rusty_core::shell_menu::ShellMenu {
                    items: rusty_ui::window_view::drop_items(|_| true),
                },
                "Drop files · Projects",
                Point::new(px(420.), px(300.)),
            ),
            Shows::Background => (
                menu_sample::folder_menu(),
                "commander · 6 items",
                Point::new(px(420.), px(330.)),
            ),
            Shows::EmptyFolder => (
                menu_sample::folder_menu(),
                "drafts · 0 items",
                Point::new(px(300.), px(150.)),
            ),
            Shows::Files => (
                menu_sample::files_menu(color),
                "3 files · 21.0 MB",
                Point::new(px(360.), px(300.)),
            ),
            _ => (
                menu_sample::zip_menu(color),
                "ferrite-0.4.2-win64.zip · 6.8 MB",
                Point::new(px(300.), px(360.)),
            ),
        };
        ContextMenuInput {
            menu,
            context: context.into(),
            anchor,
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(self.focus.clone()),
        }
    }

    fn build(&self, window: &mut Window, cx: &mut Context<Self>) -> Entity<ContextMenu> {
        let input = self.menu_input(window, cx);
        let shows = self.shows;
        cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            match shows {
                Shows::Drop => menu.hide_classic(cx),
                Shows::SevenZip => menu.open_submenu(&[SEVEN_ZIP], window, cx),
                Shows::EmptyFolder => menu.open_submenu(&[NEW], window, cx),
                Shows::Loading => menu.open_submenu(&[OPEN_WITH], window, cx),
                Shows::Failed => {
                    menu.open_submenu(&[SEND_TO], window, cx);
                    menu.set_submenu(&[SEND_TO], Submenu::Failed, cx);
                }
                Shows::Filter => menu.set_query("ext", window, cx),
                Shows::Root | Shows::Files | Shows::Background => {}
            }
            menu
        })
    }
}

impl MenuPage {
    fn open_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_none() {
            self.menu = Some(self.build(window, cx));
            cx.notify();
        }
    }
}

impl Render for MenuPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        if self.menu.is_none() {
            cx.defer_in(window, Self::open_menu);
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(t.colors.surface)
            .child(self.list())
            .children(self.menu.clone())
    }
}
