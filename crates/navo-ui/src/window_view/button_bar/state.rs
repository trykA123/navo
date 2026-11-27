//! Window-owned button bar state and latest-wins reload listener.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{FocusHandle, Subscription, Task};

use super::menu::{BarBounds, OpenButtonMenu};
use super::{ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarReceiver, ButtonItem, ItemPath};
use crate::context_menu::ContextMenu;
use crate::prelude::*;
use crate::primitives::popup_menu::{self, PopupMenu};
use crate::status_bar::StatusNotice;
use crate::window_view::NavoWindow;

/// Active body tab when the button bar editor is narrower than 820 px.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum EditorPane {
    #[default]
    Button,
    Icon,
}

/// State owned by the open button bar editor layer.
pub(crate) struct ButtonBarEditorState {
    pub(crate) selected: Option<ItemPath>,
    pub(crate) focus: FocusHandle,
    pub(crate) narrow_pane: EditorPane,
    pub(crate) error: Option<String>,
}

/// Button bar items, host hooks, focus handle, focus cursor, load error, and reload task.
pub(crate) struct ButtonBarState {
    pub(crate) items: Vec<ButtonItem>,
    pub(crate) hooks: Option<ButtonBarHooks>,
    pub(crate) focus: FocusHandle,
    pub(crate) focused: Option<usize>,
    pub(crate) editor: Option<ButtonBarEditorState>,
    pub(crate) drag_over_bar: bool,
    pub(crate) drag_hover_item: Option<usize>,
    pub(crate) error: Option<String>,
    pub(super) notice: Option<StatusNotice>,
    pub(super) context_menu: Option<(Entity<ContextMenu>, Subscription)>,
    pub(super) menu_popup: Entity<PopupMenu>,
    pub(super) open_menu: Option<OpenButtonMenu>,
    pub(super) bounds: Rc<RefCell<BarBounds>>,
    _menu_watch: Subscription,
    _news_task: Option<Task<()>>,
}

impl ButtonBarState {
    /// Creates empty button bar state with a window-owned focus handle.
    pub(crate) fn new(cx: &mut Context<NavoWindow>) -> Self {
        let menu_popup = cx.new(|cx| PopupMenu::new(cx));
        let menu_watch = cx.observe(&menu_popup, |_, _, cx| cx.notify());
        Self {
            items: Vec::new(),
            hooks: None,
            focus: cx.focus_handle(),
            focused: None,
            editor: None,
            drag_over_bar: false,
            drag_hover_item: None,
            error: None,
            notice: None,
            context_menu: None,
            menu_popup,
            open_menu: None,
            bounds: Rc::default(),
            _menu_watch: menu_watch,
            _news_task: None,
        }
    }
}

impl NavoWindow {
    /// Stores button bar hooks and starts listening for latest-wins reload news.
    pub(in crate::window_view) fn take_button_bar(
        &mut self,
        hooks: Option<ButtonBarHooks>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut hooks) = hooks else {
            self.button_bar.hooks = None;
            self.button_bar._news_task = None;
            return;
        };
        let receiver = hooks.news.take();
        self.button_bar.hooks = Some(hooks);
        if let Some(receiver) = receiver {
            self.listen_button_bar(receiver, cx);
        } else {
            self.button_bar._news_task = None;
        }
    }

    /// Spawns a cancellable listener task that applies button bar reload news to this window.
    fn listen_button_bar(&mut self, mut receiver: ButtonBarReceiver, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| {
            while let Some(news) = receiver.recv().await
                && this
                    .update(cx, |view, cx| view.apply_button_bar_news(news, cx))
                    .is_ok()
            {}
        });
        self.button_bar._news_task = Some(task);
    }

    fn apply_button_bar_news(&mut self, news: ButtonBarNews, cx: &mut Context<Self>) {
        match news {
            ButtonBarNews::Applied { items, warnings } => {
                self.button_bar.items = items;
                if self.button_bar.open_menu.take().is_some() {
                    let _ = popup_menu::dismiss(&self.button_bar.menu_popup, cx);
                }
                if self
                    .button_bar
                    .focused
                    .is_some_and(|index| index >= self.button_bar.items.len())
                {
                    self.button_bar.focused = None;
                }
                self.button_bar.error = None;
                self.button_bar.notice = warnings.into_iter().next().map(|text| StatusNotice {
                    text: text.into(),
                    is_error: false,
                });
                sync_editor_applied(&mut self.button_bar);
            }
            ButtonBarNews::Invalid(message) => {
                self.button_bar.error = Some(message.clone());
                self.button_bar.notice = Some(StatusNotice {
                    text: message.clone().into(),
                    is_error: true,
                });
                if let Some(editor) = self.button_bar.editor.as_mut() {
                    editor.error = Some(message);
                }
            }
        }
        cx.notify();
    }

    /// Returns the active button bar load warning or error notice, if any.
    pub(in crate::window_view) fn button_bar_notice(&self) -> Option<StatusNotice> {
        self.button_bar.notice.clone()
    }
}

fn sync_editor_applied(state: &mut ButtonBarState) {
    let Some(editor) = state.editor.as_mut() else {
        return;
    };
    editor.error = None;
    editor.selected = clamp_selected(&state.items, editor.selected);
}

fn clamp_selected(items: &[ButtonItem], selected: Option<ItemPath>) -> Option<ItemPath> {
    if items.is_empty() {
        return None;
    }
    let path = selected?;
    let index = path.index.min(items.len() - 1);
    let child = path.child.and_then(|child_idx| match &items[index].action {
        ButtonAction::Menu(children) if !children.is_empty() => {
            Some(child_idx.min(children.len() - 1))
        }
        _ => None,
    });
    Some(ItemPath { index, child })
}
