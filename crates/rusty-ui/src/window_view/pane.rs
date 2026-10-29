use std::rc::Rc;

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use rusty_core::{Command, PaneId, Tab};

use super::FerriteWindow;
use super::chrome_state::Hidden;
use super::clipboard::ClipboardEffect;
use super::keys::{
    BrowsePreview, DriveMenuFirst, DriveMenuSecond, EditPath, NewTab, PANE, ResetSplit,
    ShowHiddenPath, ShowHiddenTabs,
};
use super::list_columns::{column_keys, list_parts, user_widths};
use super::status::status_of;
use crate::FocusHandle;
use crate::file_list::{ClearFilter, FileList, FileListAction, FileListProps, FilterEdit};
use crate::pane_chrome::{PaneTabs, pane_card, path_segments, target_outline};
use crate::prelude::*;
use crate::settings::WindowParts;
use crate::status_bar::{FooterParts, StatusBar, StatusBarProps};
use crate::tokens;

pub(super) fn title_of(tab: &Tab) -> SharedString {
    if let Some(label) = tab.label() {
        return label.to_owned().into();
    }
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

fn footer_parts(parts: WindowParts) -> FooterParts {
    FooterParts {
        free_space: parts.free_space,
        marked_totals: parts.marked_totals,
        contents: parts.contents,
    }
}

impl FerriteWindow {
    pub(super) fn filter(&mut self, edit: FilterEdit) {
        if self.prompt.is_some()
            || matches!(edit, FilterEdit::Push(_))
                && (self.dialog_is_open() || self.dialog_input.blocked)
        {
            return;
        }
        if edit == FilterEdit::Clear && self.stop_find() {
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
        let status = self.pane_status(pane, tab, &props.rows);
        let focus = self.handle(pane, cx);
        let chrome = self.ensure_chrome(pane, window, cx);
        let parts = WindowParts::of(&self.values);
        let path = self.path_row(pane, parts, cx);
        let list = match waited {
            Some(waited) => Self::waiting_view(&waited, &focus, cx),
            None => self
                .file_list((pane, props), &focus, cx)
                .parts(list_parts(parts))
                .column_widths(user_widths(&self.values))
                .remember_width(chrome.list_width)
                .lift_above(cover)
                .into_any_element(),
        };
        let marks = self.pane_marks(pane, window);
        let header = self.pane_header(pane, marks, cx);
        let footer = self.pane_footer(pane, status, cx);
        let body = Self::pane_keys(div().key_context(PANE), cx)
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(Self::pane_swap_header(pane, (header, path), cx))
            .when_some(error, |this, message| {
                this.child(error_line(&t, pane, message))
            })
            .child(list)
            .children((!footer.is_empty()).then_some(footer));
        pane_card(("pane", pane.get()), self.card_label(pane), is_active, cx)
            .relative()
            .child(body)
            .children(marks.is_target.then(|| target_outline(&t, pane.get())))
            .child(self.pane_swap_target(pane, window, cx))
            .into_any_element()
    }

    fn pane_footer(
        &self,
        pane: PaneId,
        status: StatusBarProps,
        cx: &mut Context<Self>,
    ) -> StatusBar {
        let is_active = self.browser.state().focused_pane() == pane;
        StatusBar::new(("status", pane.get()), status)
            .parts(footer_parts(WindowParts::of(&self.values)))
            .notice(self.status_notice().filter(|_| is_active))
            .workspace(is_active.then(|| self.workspace_button(cx)).flatten())
            .summary(self.find_status(pane))
    }

    fn pane_status(&self, pane: PaneId, tab: &Tab, rows: &[rusty_fs::FsEntry]) -> StatusBarProps {
        StatusBarProps {
            free_bytes: self.free_on_drive(pane),
            mark: self.listing_mark(tab),
            hidden_marks: self.browser.hidden_marks(tab.id()),
            ..status_of(rows, tab.selection())
        }
    }

    fn list_props(&self, tab: &Tab, is_active: bool) -> FileListProps {
        FileListProps {
            folder: self.tab_title(tab),
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

    fn card_label(&self, pane: PaneId) -> &'static str {
        let panes = self.panes();
        match (panes.len(), panes.iter().position(|p| *p == pane)) {
            (2, Some(0)) => "Left pane",
            (2, Some(1)) => "Right pane",
            _ => "Pane",
        }
    }

    fn pane_keys(root: Div, cx: &mut Context<Self>) -> Div {
        let root = super::workspace_keys::workspace_keys(root, cx);
        column_keys(super::tab_keys::tab_keys(root, cx), cx)
            .on_action(cx.listener(
                |this: &mut Self, _: &super::keys::OpenTerminal, window, cx| {
                    this.open_terminal(window, cx);
                    cx.notify();
                },
            ))
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
            .on_action(cx.listener(|this: &mut Self, _: &ResetSplit, window, cx| {
                this.apply(&Command::LayoutEven, window, cx);
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
                this.escape_pane(cx);
            }))
            .on_action(
                cx.listener(|this: &mut Self, _: &ShowHiddenTabs, window, cx| {
                    this.show_hidden(Hidden::Tabs, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this: &mut Self, _: &ShowHiddenPath, window, cx| {
                    this.show_hidden(Hidden::Path, window, cx);
                }),
            )
    }

    pub(super) fn tabs(
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
        let this = cx.weak_entity();
        let tabs = tabs.on_tab_bounds(move |index, bounds, _, cx| {
            let _ = this.update(cx, |view, _| view.note_tab_bounds(pane, index, bounds));
        });
        let weak = cx.weak_entity();
        let tabs = tabs.on_menu(move |index, position, window, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.open_tab_menu(pane, index, position, window, cx)
            });
        });
        Self::wire_tab_drops(tabs, pane, cx)
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
            FileListAction::Operate(kind) => match self.browser.operate_refusal(*kind) {
                Some(refused) => self.refuse_paste(refused),
                None => {
                    self.browser.prepare(*kind);
                }
            },
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
            FileListAction::Edit { row, choose } => self.edit_row((*row, *choose), window, cx),
            FileListAction::View(row) => return self.view_or_preview(*row, window, cx),
            FileListAction::Sort(by) => {
                self.browser.sort_by(*by);
            }
            FileListAction::ResizeColumn { by, width } => {
                self.resize_column((*by, *width), window, cx)
            }
            FileListAction::ContextMenu { .. }
            | FileListAction::BackgroundMenu { .. }
            | FileListAction::MenuPrefetch { .. }
            | FileListAction::MenuCancel => {}
        }
        true
    }

    fn file_list(
        &self,
        (pane, props): (PaneId, FileListProps),
        focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> FileList {
        let find = self
            .browser
            .state()
            .pane(pane)
            .and_then(|pane| self.browser.find_results(pane.active().id()))
            .map(crate::browser::FindResults::running);
        let list = FileList::new(("list", pane.get()), props)
            .track_focus(focus)
            .find_results(find);
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
        self.step_pane(PaneStep::Next, window, cx);
    }

    pub(super) fn step_pane(
        &mut self,
        step: PaneStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browser.target_pane().is_none() {
            return;
        }
        self.close_popups(window, cx);
        let result = match step {
            PaneStep::Next => self.browser.focus_next_pane(false),
            PaneStep::Previous => self.browser.focus_next_pane(true),
            PaneStep::Target => self.browser.focus_target_pane(),
        };
        self.show_refusal(result);
        self.focus_core_pane(window, cx);
        cx.notify();
    }
}

#[derive(Clone, Copy)]
pub(super) enum PaneStep {
    Next,
    Previous,
    Target,
}
