use std::rc::Rc;

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use rusty_core::{Command, Location, PaneId, Tab};

use super::FerriteWindow;
use super::keys::{DriveMenuFirst, DriveMenuSecond, EditPath, PANE};
use super::status::status_of;
use crate::FocusHandle;
use crate::file_list::{FileList, FileListAction, FileListProps, FilterEdit};
use crate::pane_chrome::{PaneTabs, PathBar, path_segments};
use crate::prelude::*;
use crate::status_bar::StatusBar;
use crate::tokens;

fn title_of(tab: &Tab) -> SharedString {
    path_segments(tab.location())
        .pop()
        .map(|segment| segment.label)
        .unwrap_or_else(|| tab.location().as_str().to_owned().into())
}

fn error_line(t: &tokens::Theme, pane: PaneId, message: SharedString) -> impl IntoElement {
    div()
        .id(("pane-error", pane.get()))
        .role(Role::Alert)
        .aria_label(message.clone())
        .test_support()
        .px(t.spacing.xl)
        .py(t.spacing.sm)
        .text_color(t.colors.mark)
        .child(message)
}

impl FerriteWindow {
    pub(super) fn filter(&mut self, edit: FilterEdit) {
        if self.prompt.is_some() {
            return;
        }
        let _ = match edit {
            FilterEdit::Push(ch) => self.browser.filter_push(ch),
            FilterEdit::Pop => self.browser.filter_pop(),
            FilterEdit::Clear => self.browser.filter_clear(),
        };
    }

    pub(super) fn pane_view(
        &mut self,
        pane: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = tokens::current(cx).clone();
        let state = self.browser.state();
        let Some(core_pane) = state.pane(pane) else {
            return div();
        };
        let tab = core_pane.active();
        let waiting = self.waiting.is_waiting(tab).then(|| tab.clone());
        let titles: Vec<SharedString> = core_pane.tabs().iter().map(title_of).collect();
        let active_index = core_pane.active_index();
        let is_active = state.focused_pane() == pane;
        let rows = self.browser.details(tab.id());
        let error = self
            .drives
            .error(pane, tab.location())
            .or_else(|| self.browser.error(tab.id()).map(|e| e.message().into()));
        let status = status_of(&rows, tab.selection());
        let props = FileListProps {
            folder: title_of(tab),
            rows,
            view: tab.view(),
            marked: Rc::new(tab.selection().clone()),
            is_active,
            filter: self
                .browser
                .filter_text(tab.id())
                .map(|text| text.to_owned().into()),
            sort: self.browser.sort_key(tab.id()),
        };
        let focus = self.handle(pane, cx);
        let _ = window;
        let path = self.path_row(pane, cx);
        let list = match waiting {
            Some(tab) => Self::waiting_view(pane, &tab, &focus, cx),
            None => Self::file_list(pane, props, &focus, cx).into_any_element(),
        };
        Self::pane_keys(div().key_context(PANE), cx)
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(Self::tabs(pane, titles, active_index, cx))
            .child(self.drive_row(pane, cx))
            .child(path)
            .when_some(error, |this, message| {
                this.child(error_line(&t, pane, message))
            })
            .child(list)
            .child(StatusBar::new(("status", pane.get()), status))
    }

    fn drive_row(&self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        div()
            .relative()
            .flex()
            .flex_col()
            .child(self.drive_bar(pane, cx))
            .children(self.drive_menu_view(pane, cx))
    }

    fn path_row(&self, pane: PaneId, cx: &mut Context<Self>) -> AnyElement {
        if let Some(input) = self.path_input(pane, cx) {
            return input.into_any_element();
        }
        let state = self.browser.state();
        let is_active = state.focused_pane() == pane;
        let location = state
            .pane(pane)
            .map(|p| p.active().location().clone())
            .unwrap_or_else(|| state.active_tab().location().clone());
        Self::path_bar(pane, location, is_active, cx).into_any_element()
    }

    fn pane_keys(root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(
            cx.listener(|this: &mut Self, _: &DriveMenuFirst, window, cx| {
                this.open_drive_menu(0, window, cx);
            }),
        )
        .on_action(
            cx.listener(|this: &mut Self, _: &DriveMenuSecond, window, cx| {
                this.open_drive_menu(1, window, cx);
            }),
        )
        .on_action(cx.listener(|this: &mut Self, _: &EditPath, _, cx| {
            let pane = this.browser.state().focused_pane();
            this.open_path_edit(pane, cx);
        }))
    }

    fn tabs(
        pane: PaneId,
        titles: Vec<SharedString>,
        active: usize,
        cx: &mut Context<Self>,
    ) -> PaneTabs {
        PaneTabs::new(("tabs", pane.get()), titles, active).on_command(cx.listener(
            move |this: &mut Self, command: &Command, window, cx| {
                this.focus_pane(pane);
                this.apply(command, window, cx);
            },
        ))
    }

    fn path_bar(
        pane: PaneId,
        location: Location,
        is_active: bool,
        cx: &mut Context<Self>,
    ) -> PathBar {
        let edit = cx.weak_entity();
        PathBar::new(("path", pane.get()), location)
            .active(is_active)
            .on_navigate(
                cx.listener(move |this: &mut Self, location: &Location, window, cx| {
                    let command = Command::PaneNavigate {
                        location: location.clone(),
                    };
                    this.focus_pane(pane);
                    this.apply(&command, window, cx);
                }),
            )
            .on_edit(move |_, cx| {
                let _ = edit.update(cx, |view, cx| {
                    view.focus_pane(pane);
                    view.open_path_edit(pane, cx);
                });
            })
    }

    fn file_list(
        pane: PaneId,
        props: FileListProps,
        focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> FileList {
        FileList::new(("list", pane.get()), props)
            .track_focus(focus)
            .on_command(
                cx.listener(move |this: &mut Self, command: &Command, window, cx| {
                    this.focus_pane(pane);
                    this.apply(command, window, cx);
                }),
            )
            .on_list_action(cx.listener(
                move |this: &mut Self, action: &FileListAction, window, cx| {
                    this.focus_pane(pane);
                    match action {
                        FileListAction::Open(row) => {
                            this.browser.open_row(*row);
                        }
                        FileListAction::Up => {
                            this.browser.go_up();
                        }
                        FileListAction::SwitchPane => this.switch_pane(window, cx),
                        FileListAction::Operate(kind) => {
                            this.browser.prepare(*kind);
                        }
                        FileListAction::Filter(edit) => {
                            this.filter(*edit);
                        }
                        FileListAction::Rename(row) => this.ask_rename(*row),
                        FileListAction::NewFolder => this.ask_new_folder(),
                        FileListAction::SelectPattern { add } => this.ask_pattern(*add),
                        FileListAction::Edit { row, choose } => this.edit_row(*row, *choose),
                        FileListAction::View(row) => this.view_row(*row, window, cx),
                        FileListAction::Sort(by) => {
                            this.browser.sort_by(*by);
                        }
                    }
                    this.focus_core_pane(window, cx);
                    cx.notify();
                },
            ))
            .on_page_rows(cx.listener(move |this: &mut Self, rows: &u64, _, cx| {
                this.browser
                    .event(rusty_core::Event::PageRowsChanged { pane, rows: *rows });
                cx.notify();
            }))
    }
}
