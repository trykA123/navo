//! Window-owned button bar state and latest-wins reload listener.

use gpui_kit::{FocusHandle, Task};

use super::{ButtonBarHooks, ButtonBarNews, ButtonBarReceiver, ButtonItem};
use crate::prelude::*;
use crate::status_bar::StatusNotice;
use crate::window_view::NavoWindow;

/// Button bar items, host hooks, focus handle, focus cursor, load error, and reload task.
pub(crate) struct ButtonBarState {
    pub(crate) items: Vec<ButtonItem>,
    pub(crate) hooks: Option<ButtonBarHooks>,
    pub(crate) focus: FocusHandle,
    pub(crate) focused: Option<usize>,
    pub(crate) error: Option<String>,
    pub(super) notice: Option<StatusNotice>,
    _news_task: Option<Task<()>>,
}

impl ButtonBarState {
    /// Creates empty button bar state with a window-owned focus handle.
    pub(crate) fn new(cx: &mut Context<NavoWindow>) -> Self {
        Self {
            items: Vec::new(),
            hooks: None,
            focus: cx.focus_handle(),
            focused: None,
            error: None,
            notice: None,
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
            }
            ButtonBarNews::Invalid(message) => {
                self.button_bar.error = Some(message.clone());
                self.button_bar.notice = Some(StatusNotice {
                    text: message.into(),
                    is_error: true,
                });
            }
        }
        cx.notify();
    }

    /// Returns the active button bar load warning or error notice, if any.
    pub(in crate::window_view) fn button_bar_notice(&self) -> Option<StatusNotice> {
        self.button_bar.notice.clone()
    }
}
