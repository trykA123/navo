//! The Stream's search field (V29, D30): `/` opens it above the cards, typing narrows the Stream
//! after a 150 ms pause, Enter hands focus to the cards, Esc clears and closes it.

use std::time::Duration;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Role, Stateful, Subscription, Task};
use navo_l10n::{t, tr};

mod rerun;

use super::sheet;
use super::stream::StreamScreen;
use crate::commands::compare::{Exit, Search as OpenSearch};
use crate::prelude::*;
use crate::primitives::{IconButton, IconName, TextField, TextFieldState, icon, text_field_state};
use crate::tokens::Theme;
use crate::window_view::NavoWindow;
pub(super) use rerun::Rerun;

/// How long typing rests before the query goes to the app (D30).
const DEBOUNCE: Duration = Duration::from_millis(150);
/// Width of the field, in px.
const FIELD_WIDTH: f32 = 300.;

/// The field, its subscription and what was last sent to the app.
pub(super) struct Search {
    input: Entity<TextFieldState>,
    _events: Subscription,
    open: bool,
    /// The tag of the last query sent; each typed text gets a new one.
    query: u64,
    /// The text of the last query sent; empty when none runs.
    sent: String,
    debounce: Option<Task<()>>,
}

impl StreamScreen {
    /// Whether the search field is shown.
    pub(super) fn search_open(&self) -> bool {
        self.search.as_ref().is_some_and(|search| search.open)
    }

    /// The text in the field.
    fn search_text(&self, cx: &App) -> String {
        self.search
            .as_ref()
            .map_or_else(String::new, |s| s.input.read(cx).value().to_string())
    }

    /// Sends the field's text as a new query, or stops the search when it is empty. Does
    /// nothing when the text is what the app already searches.
    fn search_send(&mut self, cx: &App) {
        let text = self.search_text(cx);
        let Some(search) = self.search.as_mut() else {
            return;
        };
        if text == search.sent {
            return;
        }
        search.sent.clone_from(&text);
        search.query += 1;
        let query = search.query;
        if text.is_empty() {
            self.model.clear_search();
        } else {
            self.model.begin_search(query);
        }
        self.control.search(query, text);
        self.sync_rows();
    }

    /// The item list changed: the app dropped its search, so asks it again under a new tag.
    /// The hits on screen stay until the new ones replace them.
    /// False when no search runs.
    pub(super) fn search_rerun(&mut self) -> bool {
        let Some(search) = self.search.as_mut().filter(|s| !s.sent.is_empty()) else {
            return false;
        };
        search.query += 1;
        self.model.restart_search(search.query);
        self.control.search(search.query, search.sent.clone());
        true
    }

    /// Closes the field: stops the search, clears the field and the narrowing.
    fn search_end(&mut self, window: &mut Window, cx: &mut App) {
        let Some(search) = self.search.as_mut() else {
            return;
        };
        search.open = false;
        search.debounce = None;
        search.input.update(cx, |state, cx| {
            state.set_value(String::new(), window, cx);
        });
        self.search_send(cx);
        self.model.clear_search();
        self.sync_rows();
    }

    /// The note for an empty list: no files at all, or a finished search with no match.
    pub(super) fn empty_note(&self) -> Option<SharedString> {
        if self.model.items().is_empty() {
            return Some(tr("compare-stream-empty").into());
        }
        let none = self.model.search_done() && self.model.visible().is_empty();
        none.then(|| tr("compare-search-none").into())
    }
}

/// The count line: "N changes in M files match", then "· Searching… n of N" while it runs.
fn status_text(stream: &StreamScreen) -> SharedString {
    let (changes, files) = stream.model.match_counts();
    let found = t!("compare-search-found", changes = changes, files = files);
    match stream.model.search_progress() {
        Some((searched, total)) => format!(
            "{found} · {}",
            t!(
                "compare-search-progress",
                searched = searched,
                total = total
            )
        )
        .into(),
        None => found.into(),
    }
}

/// The search row between the header and the cards, or `None` while the field is closed.
pub(super) fn bar(
    t: &Theme,
    stream: &StreamScreen,
    cx: &mut Context<NavoWindow>,
) -> Option<AnyElement> {
    let search = stream.search.as_ref().filter(|s| s.open)?;
    let clear = cx.listener(|view, _: &ClickEvent, window, cx| view.search_close(window, cx));
    let status = div()
        .id("compare-search-status")
        .role(Role::Status)
        .test_support()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .when(stream.model.searching(), |d| d.child(status_text(stream)));
    let row = div()
        .id("compare-search")
        .on_action(cx.listener(|view, _: &Exit, window, cx| view.search_close(window, cx)))
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.md)
        .h(t.spacing.path_height)
        .border_b_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .child(icon(IconName::Search, t.spacing.icon, t.colors.muted))
        .child(div().flex_none().w(px(FIELD_WIDTH)).child(TextField::new(
            "compare-search-input",
            &search.input,
            tr("compare-search-label"),
        )))
        .child(status)
        .children(sheet::buttons(t, stream, cx))
        .child(
            IconButton::new(
                "compare-search-clear",
                IconName::X,
                tr("compare-search-clear"),
            )
            .on_click(clear),
        );
    Some(row.into_any_element())
}

impl NavoWindow {
    /// Binds `/` (open the search field) on the Stream's `root`.
    pub(super) fn search_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &OpenSearch, window, cx| {
            view.search_show(window, cx);
        }))
    }

    /// `/`: shows the field, creating it on first use, and focuses it.
    fn search_show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stream) = self.stream_mut() else {
            return;
        };
        if stream.search.is_none() {
            let input = text_field_state(tr("compare-search-label"), window, cx);
            let events =
                cx.subscribe_in(&input, window, |view, _, event: &InputEvent, window, cx| {
                    view.search_event(event, window, cx);
                });
            let Some(stream) = self.stream_mut() else {
                return;
            };
            stream.search = Some(Search {
                input,
                _events: events,
                open: false,
                query: 0,
                sent: String::new(),
                debounce: None,
            });
        }
        let Some(search) = self.stream_mut().and_then(|s| s.search.as_mut()) else {
            return;
        };
        search.open = true;
        let focus = search.input.read(cx).focus_handle(cx);
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        cx.notify();
    }

    /// Typing restarts the debounce; Enter hands focus to the cards at the first match.
    fn search_event(&mut self, event: &InputEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            InputEvent::Change => self.search_debounce(cx),
            InputEvent::PressEnter { .. } => {
                if let Some(stream) = self.stream_mut() {
                    stream.reveal_cursor();
                    let focus = stream.focus.clone();
                    window.focus(&focus, cx);
                }
                cx.notify();
            }
            _ => {}
        }
    }

    /// Waits 150 ms after the last change, then sends the query; a newer change replaces the wait.
    fn search_debounce(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let _ = this.update(cx, Self::search_due);
        });
        if let Some(search) = self.stream_mut().and_then(|s| s.search.as_mut()) {
            search.debounce = Some(task);
        }
    }

    /// The pause ended: sends the typed text and shows the first match.
    fn search_due(&mut self, cx: &mut Context<Self>) {
        if let Some(stream) = self.stream_mut() {
            stream.search_send(cx);
            stream.reveal_cursor();
            cx.notify();
        }
    }

    /// Esc or Clear: closes the field, stops the search and gives focus back to the cards.
    pub(super) fn search_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stream) = self.stream_mut() else {
            return;
        };
        stream.search_end(window, cx);
        let focus = stream.focus.clone();
        window.focus(&focus, cx);
        cx.notify();
    }
}

#[cfg(test)]
impl NavoWindow {
    /// Whether the field is shown, its text, and the (tag, text) of the last query sent.
    pub(in crate::window_view) fn search_probe(&self, cx: &App) -> Option<(bool, String, u64)> {
        let stream = self.compare.screen.as_ref()?.stream.as_ref()?;
        let search = stream.search.as_ref()?;
        Some((search.open, stream.search_text(cx), search.query))
    }

    /// Gives the Stream a new item list, as a compare update would.
    pub(in crate::window_view) fn stream_replace(
        &mut self,
        items: (
            crate::compare_diff::StreamList,
            Vec<crate::compare_diff::PairRequest>,
        ),
        fresh: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self
            .stream_mut()
            .is_some_and(|stream| stream.replace_items(items, fresh));
        self.arm_rerun(cx);
        changed
    }
}
