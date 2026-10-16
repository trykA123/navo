use std::rc::Rc;

use rusty_core::{Selection, ViewState};
use rusty_fs::FsEntry;
use rusty_ui::FocusHandle;
use rusty_ui::context_menu::{ContextMenu, ContextMenuInput};
use rusty_ui::file_list::{FileList, FileListProps, SortKey};
use rusty_ui::prelude::*;
use rusty_ui::tokens;

use crate::menu_sample::{self, SEVEN_ZIP};
use crate::pane_sample;

const ZIP_ROW: u64 = 11;
const MARKED: [u64; 3] = [6, 7, 9];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shows {
    Root,
    SevenZip,
    Filter,
    Files,
}

pub struct MenuPage {
    shows: Shows,
    rows: Rc<[FsEntry]>,
    focus: FocusHandle,
    menu: Option<Entity<ContextMenu>>,
}

impl MenuPage {
    pub fn new(shows: Shows, cx: &mut Context<Self>) -> Self {
        Self {
            shows,
            rows: pane_sample::rows(),
            focus: cx.focus_handle(),
            menu: None,
        }
    }

    fn list(&self) -> FileList {
        let mut marked = Selection::new(self.rows.len() as u64);
        let cursor = if self.shows == Shows::Files {
            marked.mark_rows(MARKED);
            MARKED[2]
        } else {
            ZIP_ROW
        };
        let props = FileListProps {
            folder: "commander".into(),
            rows: self.rows.clone(),
            view: ViewState { cursor, scroll: 0 },
            marked: Rc::new(marked),
            is_active: true,
            filter: None,
            sort: SortKey::default(),
        };
        FileList::new("menu-page-list", props).track_focus(&self.focus)
    }

    fn build(&self, window: &mut Window, cx: &mut Context<Self>) -> Entity<ContextMenu> {
        let color = tokens::current(cx).colors.kind_archive;
        let (menu, context, anchor) = match self.shows {
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
        let input = ContextMenuInput {
            menu,
            context: context.into(),
            anchor,
            bounds: Bounds::new(Point::default(), window.viewport_size()),
        };
        let shows = self.shows;
        cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            match shows {
                Shows::SevenZip => menu.open_submenu(&[SEVEN_ZIP], cx),
                Shows::Filter => menu.set_query("ext", window, cx),
                Shows::Root | Shows::Files => {}
            }
            menu
        })
    }
}

impl Render for MenuPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let menu = match &self.menu {
            Some(menu) => menu.clone(),
            None => {
                let menu = self.build(window, cx);
                self.menu = Some(menu.clone());
                menu
            }
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(t.colors.surface)
            .child(self.list())
            .child(menu)
    }
}
