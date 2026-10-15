use std::rc::Rc;

use rusty_core::{Command, Location, PaneId, Tab};

use super::FerriteWindow;
use super::status::status_of;
use crate::FocusHandle;
use crate::file_list::{FileList, FileListAction, FileListProps};
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

impl FerriteWindow {
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
        let titles: Vec<SharedString> = core_pane.tabs().iter().map(title_of).collect();
        let active_index = core_pane.active_index();
        let is_active = state.focused_pane() == pane;
        let location = tab.location().clone();
        let rows = self.browser.details(tab.id());
        let error = self.browser.error(tab.id()).map(|e| e.message());
        let status = status_of(&rows, tab.selection());
        let props = FileListProps {
            folder: title_of(tab),
            rows,
            view: tab.view(),
            marked: Rc::new(tab.selection().clone()),
            is_active,
        };
        let focus = self.handle(pane, cx);
        let _ = window;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(Self::tabs(pane, titles, active_index, cx))
            .child(Self::path_bar(pane, location, is_active, cx))
            .when_some(error, |this, message| {
                this.child(
                    div()
                        .px(t.spacing.xl)
                        .py(t.spacing.sm)
                        .text_color(t.colors.mark)
                        .child(message),
                )
            })
            .child(Self::file_list(pane, props, &focus, cx))
            .child(StatusBar::new(("status", pane.get()), status))
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
