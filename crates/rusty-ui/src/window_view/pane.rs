use std::rc::Rc;

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use rusty_core::{Command, Location, PaneId, Tab};

use super::FerriteWindow;
use super::clipboard::ClipboardEffect;
use super::keys::{
    BrowsePreview, DriveMenuFirst, DriveMenuSecond, EditPath, NewTab, PANE, ResetSplit,
};
use super::status::status_of;
use crate::FocusHandle;
use crate::file_list::{
    ClearFilter, FileList, FileListAction, FileListProps, FilterEdit, ListParts,
};
use crate::pane_chrome::{PaneTabs, PathBar, card_header, pane_card, path_segments};
use crate::prelude::*;
use crate::settings::WindowParts;
use crate::status_bar::{FooterParts, StatusBar, StatusBarProps};
use crate::tokens;

pub(super) fn title_of(tab: &Tab) -> SharedString {
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

fn list_parts(parts: WindowParts) -> ListParts {
    ListParts {
        ext: parts.ext_column,
        size: parts.size_column,
        date: parts.date_column,
        icons: parts.icons,
    }
}

fn footer_parts(parts: WindowParts) -> FooterParts {
    FooterParts {
        free_space: parts.free_space,
        marked_totals: parts.marked_totals,
        contents: parts.contents,
    }
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

    fn pane_error(&self, pane: PaneId, tab: &Tab) -> Option<SharedString> {
        let is_focused = self.browser.state().focused_pane() == pane;
        self.drives
            .error(pane, tab)
            .or_else(|| self.browser.error(tab.id()).map(|e| e.message().into()))
            .or_else(|| self.saver.error.clone().filter(|_| is_focused))
    }

    pub(super) fn pane_view(
        &mut self,
        pane: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = tokens::current(cx).clone();
        let label = self.card_label(pane);
        let state = self.browser.state();
        let Some(core_pane) = state.pane(pane) else {
            return div().into_any_element();
        };
        let tab = core_pane.active();
        let waited = self.waited(pane, tab);
        let is_active = state.focused_pane() == pane;
        let error = self.pane_error(pane, tab);
        let has_cursor = is_active && !self.preview.is_browsing(window, cx);
        let props = self.list_props(tab, has_cursor);
        let cover = self.hint_cover(window, cx).filter(|_| has_cursor);
        let status = StatusBarProps {
            free_bytes: self.free_on_drive(pane),
            mark: self.listing_mark(tab),
            hidden_marks: self.browser.hidden_marks(tab.id()),
            ..status_of(&props.rows, tab.selection())
        };
        let focus = self.handle(pane, cx);
        let parts = WindowParts::of(&self.values);
        let path = self.path_row(pane, parts, cx);
        let list = match waited {
            Some(waited) => Self::waiting_view(&waited, &focus, cx),
            None => Self::file_list(pane, props, &focus, cx)
                .parts(list_parts(parts))
                .lift_above(cover)
                .into_any_element(),
        };
        let header = self.pane_header(pane, is_active, cx);
        let footer = StatusBar::new(("status", pane.get()), status)
            .parts(footer_parts(parts))
            .notice(self.status_notice().filter(|_| is_active));
        let body = Self::pane_keys(div().key_context(PANE), cx)
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .children(header)
            .children(path)
            .when_some(error, |this, message| {
                this.child(error_line(&t, pane, message))
            })
            .child(list)
            .children((!footer.is_empty()).then_some(footer));
        pane_card(("pane", pane.get()), label, is_active, cx)
            .child(body)
            .into_any_element()
    }

    fn list_props(&self, tab: &Tab, is_active: bool) -> FileListProps {
        FileListProps {
            folder: title_of(tab),
            rows: self.browser.details(tab.id()),
            view: tab.view(),
            marked: Rc::new(tab.selection().clone()),
            is_active,
            filter: self
                .browser
                .filter_text(tab.id())
                .map(|text| text.to_owned().into()),
            sort: self.browser.sort_key(tab.id()),
        }
    }

    fn pane_header(&self, pane: PaneId, is_active: bool, cx: &mut Context<Self>) -> Option<Div> {
        let t = tokens::current(cx).clone();
        let parts = WindowParts::of(&self.values);
        let (titles, active_index): (Vec<SharedString>, usize) = self
            .browser
            .state()
            .pane(pane)
            .map(|p| (p.tabs().iter().map(title_of).collect(), p.active_index()))
            .unwrap_or_default();
        let shows_tabs = parts.tabs_with_one || titles.len() > 1;
        let drive = self.drive_pill(pane, parts.drive_button, cx);
        if !shows_tabs && !parts.drive_button {
            let is_menu_open = self.drive_menu.is_some();
            return is_menu_open.then(|| div().relative().h_0().flex_none().child(drive));
        }
        let tabs = shows_tabs.then(|| {
            let tabs = Self::tabs(pane, titles, active_index, cx).highlighted(is_active);
            div().flex_1().min_w_0().child(tabs)
        });
        let header = card_header(&t).children(tabs);
        Some(match shows_tabs {
            true => header.child(drive),
            false => header.justify_end().child(drive),
        })
    }

    fn card_label(&self, pane: PaneId) -> &'static str {
        let panes = self.panes();
        match (panes.len(), panes.iter().position(|p| *p == pane)) {
            (2, Some(0)) => "Left pane",
            (2, Some(1)) => "Right pane",
            _ => "Pane",
        }
    }

    fn path_row(
        &self,
        pane: PaneId,
        parts: WindowParts,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(input) = self.path_input(pane, cx) {
            return Some(input.into_any_element());
        }
        if !parts.path_bar {
            return None;
        }
        let state = self.browser.state();
        let location = state
            .pane(pane)
            .map(|p| p.active().location().clone())
            .unwrap_or_else(|| state.active_tab().location().clone());
        Some(Self::path_bar(pane, location, cx).into_any_element())
    }

    fn pane_keys(root: Div, cx: &mut Context<Self>) -> Div {
        super::tab_keys::tab_keys(root, cx)
            .on_action(
                cx.listener(|this: &mut Self, _: &DriveMenuFirst, window, cx| {
                    this.open_drive_menu(0, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this: &mut Self, _: &DriveMenuSecond, window, cx| {
                    this.open_drive_menu(1, window, cx);
                }),
            )
            .on_action(cx.listener(|this: &mut Self, _: &NewTab, window, cx| {
                this.apply(&Command::PaneOpenTab { location: None }, window, cx);
            }))
            .on_action(cx.listener(|this: &mut Self, _: &ResetSplit, _, cx| {
                this.split.update(cx, |split, cx| {
                    split.reset();
                    cx.notify();
                });
            }))
            .on_action(cx.listener(|this: &mut Self, _: &EditPath, _, cx| {
                let pane = this.browser.state().focused_pane();
                this.open_path_edit(pane, cx);
            }))
            .on_action(
                cx.listener(|this: &mut Self, _: &BrowsePreview, window, cx| {
                    this.browse_preview(window, cx);
                }),
            )
            .on_action(cx.listener(|this: &mut Self, _: &ClearFilter, _, cx| {
                this.escape_preview(cx);
            }))
    }

    fn tabs(
        pane: PaneId,
        titles: Vec<SharedString>,
        active: usize,
        cx: &mut Context<Self>,
    ) -> PaneTabs {
        let tabs = PaneTabs::new(("tabs", pane.get()), titles, active).on_command(cx.listener(
            move |this: &mut Self, command: &Command, window, cx| {
                this.focus_pane(pane);
                this.apply(command, window, cx);
            },
        ));
        Self::wire_tab_drops(tabs, pane, cx)
    }

    fn path_bar(pane: PaneId, location: Location, cx: &mut Context<Self>) -> PathBar {
        let edit = cx.weak_entity();
        let bar = PathBar::new(("path", pane.get()), location);
        Self::wire_crumb_drops(bar, pane, cx)
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

    fn list_action(
        &mut self,
        action: &FileListAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            FileListAction::Open(row) => {
                self.browser.open_row(*row);
                self.note_seen(cx);
            }
            FileListAction::Up => {
                self.browser.go_up();
            }
            FileListAction::SwitchPane => self.switch_pane(window, cx),
            FileListAction::Operate(kind) => {
                self.browser.prepare(*kind);
            }
            FileListAction::CopyToClipboard => self.put_on_clipboard(ClipboardEffect::Copy),
            FileListAction::CutToClipboard => self.put_on_clipboard(ClipboardEffect::Move),
            FileListAction::Paste => self.paste(),
            FileListAction::Filter(edit) => {
                self.filter(*edit);
            }
            FileListAction::Rename(row) => self.rename_row(*row, window, cx),
            FileListAction::NewFolder => self.ask_new_folder(),
            FileListAction::NewFile => self.ask_new_file(),
            FileListAction::CopyPath => self.copy_full_paths(cx),
            FileListAction::SelectPattern { add } => self.ask_pattern(*add),
            FileListAction::Edit { row, choose } => self.edit_row(*row, *choose),
            FileListAction::View(row) => return self.view_or_preview(*row, window, cx),
            FileListAction::Sort(by) => {
                self.browser.sort_by(*by);
            }
            FileListAction::ContextMenu { .. }
            | FileListAction::BackgroundMenu { .. }
            | FileListAction::MenuPrefetch { .. }
            | FileListAction::MenuCancel => {}
        }
        true
    }

    fn file_list(
        pane: PaneId,
        props: FileListProps,
        focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> FileList {
        let list = FileList::new(("list", pane.get()), props).track_focus(focus);
        Self::wire_drops(list, pane, cx)
            .on_command(
                cx.listener(move |this: &mut Self, command: &Command, window, cx| {
                    this.focus_pane(pane);
                    this.apply(command, window, cx);
                }),
            )
            .on_list_action(cx.listener(
                move |this: &mut Self, action: &FileListAction, window, cx| {
                    this.focus_pane(pane);
                    if this.route_menu_action((pane, action), window, cx) {
                        return;
                    }
                    if this.list_action(action, window, cx) {
                        this.focus_core_pane(window, cx);
                    }
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

impl FerriteWindow {
    pub(super) fn panes(&self) -> Vec<PaneId> {
        let state = self.browser.state();
        state
            .window(state.focused_window())
            .map(|w| w.panes().iter().map(|p| p.id()).collect())
            .unwrap_or_default()
    }

    pub(super) fn switch_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.browser.state().focused_pane();
        if let Some(other) = self.panes().into_iter().find(|p| *p != focused) {
            self.focus_pane(other);
            self.focus_core_pane(window, cx);
            cx.notify();
        }
    }
}
