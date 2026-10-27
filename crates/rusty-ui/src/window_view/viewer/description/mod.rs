mod outline;

use std::any::Any;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};

use gpui_kit::base::TestSupportExt;
use gpui_kit::{AnyView, FocusHandle, KeyDownEvent, Role, Stateful};
use rusty_automotive::{A2lModel, DbcModel, Outline, a2l, a2l::Links, dbc};
use rusty_l10n::t;

use super::LeftDrawer;
use super::body::{BodyKeys, Host, ViewerBody};
use super::parts::{AUTOMOTIVE_OUTLINE, PartSet};
use super::rich_header::{HeaderParts, header};
use crate::prelude::*;
use crate::primitives::{ListScroll, VirtualList};
use crate::settings::Values;
use crate::tokens::{self, Theme};
use outline::OutlineState;

type Listener<E> = Rc<dyn Fn(&E, &mut Window, &mut App)>;
type Bind = Box<dyn Fn(Stateful<Div>) -> Stateful<Div>>;

pub(super) enum DescModel {
    Dbc(DbcModel),
    A2l(A2lModel, Links),
}

pub(super) struct DescriptionFile {
    pub len: u64,
    pub outline: Arc<Outline>,
    pub model: Arc<DescModel>,
}

impl DescriptionFile {
    pub(super) fn new(len: u64, model: DescModel) -> Self {
        let outline = match &model {
            DescModel::Dbc(model) => dbc::outline(model),
            DescModel::A2l(model, _links) => a2l::outline(model),
        };
        Self {
            len,
            outline: Arc::new(outline),
            model: Arc::new(model),
        }
    }
}

struct DescriptionView {
    title: SharedString,
    file: DescriptionFile,
    focus: FocusHandle,
    scroll: ListScroll,
    outline: OutlineState,
    outline_open: bool,
    inspector_open: bool,
    note: Option<SharedString>,
    bindings: Vec<Bind>,
    on_key: Listener<KeyDownEvent>,
    row_clicks: Vec<Listener<ClickEvent>>,
}

impl DescriptionView {
    fn new(file: DescriptionFile, host: Host, cx: &mut Context<Self>) -> Self {
        let row_clicks = (0..file.outline.rows.len())
            .map(|at| {
                Rc::new(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    view.click_row(at, window, cx);
                })) as Listener<ClickEvent>
            })
            .collect();
        Self {
            title: host.title,
            outline: OutlineState::new(file.outline.clone()),
            file,
            focus: cx.focus_handle(),
            scroll: ListScroll::new(),
            outline_open: true,
            inspector_open: false,
            note: None,
            bindings: outline::bindings(cx),
            on_key: Rc::new(cx.listener(Self::type_key)),
            row_clicks,
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(at) = self.outline.position() {
            self.scroll.scroll_to(at);
        }
        self.note = None;
        cx.notify();
    }

    fn toggle_outline(&mut self, cx: &mut Context<Self>) {
        self.outline_open = !self.outline_open;
        if let Some(at) = self.outline.position() {
            self.scroll.scroll_to(at);
        }
        cx.notify();
    }

    fn type_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers != gpui_kit::Modifiers::default() {
            return;
        }
        match event.keystroke.key.as_str() {
            "j" => self.outline.move_by(1),
            "k" => self.outline.move_by(-1),
            "home" => self.outline.move_by(isize::MIN),
            "end" => self.outline.move_by(isize::MAX),
            _ => return,
        }
        cx.stop_propagation();
        self.refresh(cx);
    }

    fn outline_panel(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.weak_entity();
        let list = VirtualList::new(
            "description-list",
            self.outline.visible.len(),
            &self.scroll,
            move |range, _, cx| {
                let Some(view) = view.upgrade() else {
                    return Vec::new();
                };
                let t = tokens::current(cx);
                let view = view.read(cx);
                range
                    .map(|at| view.row(view.outline.visible[at], t))
                    .collect()
            },
        )
        .label(t!("viewer-description-outline"));
        div()
            .id("description-outline")
            .role(Role::Tree)
            .aria_label(t!("viewer-description-outline"))
            .test_support()
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .when(self.inspector_open, |panel| {
                panel.w(t.spacing.rail_width * 1.5).max_w(relative(0.45))
            })
            .when(!self.inspector_open, |panel| panel.flex_1())
            .child(list)
            .into_any_element()
    }

    fn body(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .when(self.outline_open, |body| {
                body.child(self.outline_panel(t, cx))
            })
            .when(self.inspector_open, |body| {
                body.child(self.inspector_panel(t))
            })
    }

    fn inspector_panel(&self, t: &Theme) -> AnyElement {
        let prompt = match self.file.model.as_ref() {
            DescModel::Dbc(_) => t!("viewer-description-pick-dbc"),
            DescModel::A2l(_, _) => t!("viewer-description-pick-a2l"),
        };
        div()
            .id("description-inspector")
            .aria_label(prompt.clone())
            .test_support()
            .flex_1()
            .min_w_0()
            .p(t.spacing.xxl)
            .border_l_1()
            .border_color(t.colors.line)
            .text_color(t.colors.muted)
            .child(prompt)
            .into_any_element()
    }
}

impl Render for DescriptionView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let root = div()
            .id("description")
            .key_context("Description")
            .track_focus(&self.focus);
        let root = self.bindings.iter().fold(root, |root, bind| bind(root));
        let on_key = self.on_key.clone();
        let mode = match self.file.model.as_ref() {
            DescModel::Dbc(_) => "DBC",
            DescModel::A2l(_, _) => "A2L",
        };
        root.on_key_down(move |event, window, cx| on_key(event, window, cx))
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(t.colors.surface)
            .text_color(t.colors.text)
            .text_size(t.typography.body)
            .child(header(
                &t,
                HeaderParts {
                    title: self.title.clone(),
                    mode: mode.into(),
                    len: self.file.len,
                    detail: None,
                    status: self.note.clone().map(|note| ("description-status", note)),
                },
            ))
            .child(self.body(&t, cx))
    }
}

pub(super) struct DescriptionBody(Entity<DescriptionView>);

impl DescriptionBody {
    pub(super) fn new(file: DescriptionFile, host: Host, cx: &mut App) -> Self {
        Self(cx.new(|cx| DescriptionView::new(file, host, cx)))
    }
}

impl ViewerBody for DescriptionBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus.clone()
    }

    fn keys(&self, _: &App) -> BodyKeys {
        static LABELS: LazyLock<[String; 5]> = LazyLock::new(|| {
            [
                t!("viewer-description-move"),
                t!("viewer-description-fold"),
                t!("viewer-description-toggle-group"),
                t!("viewer-description-outline"),
                t!("viewer-description-inspector"),
            ]
        });
        BodyKeys::Normal(
            ["↑/↓ J/K", "←/→", "Enter", "Ctrl+B", "Ctrl+I"]
                .into_iter()
                .zip(LABELS.iter().map(String::as_str))
                .collect(),
        )
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| {
            view.note = Some(note);
            cx.notify();
        });
    }

    fn takes_letters(&self, _: &App) -> bool {
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn parts(&self, _: &App) -> Option<&'static PartSet> {
        Some(&AUTOMOTIVE_OUTLINE)
    }

    fn show_parts(&self, values: &Values, cx: &mut App) {
        self.0.update(cx, |view, cx| {
            let shown = values.opens_inspector();
            if view.inspector_open != shown {
                view.inspector_open = shown;
                cx.notify();
            }
        });
    }
}

#[cfg(test)]
mod tests;
