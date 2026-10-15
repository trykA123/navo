use gpui_kit::base::TestSupportExt;
use gpui_kit::{BoxShadow, ClickEvent, WeakEntity, point};
use rusty_structured::{Flavor, Key, NodeId, NodeKind, Row, Tree};

use super::TreeView;
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::{Colors, Theme};

const INDENT: f32 = 16.;

pub(super) struct RowLook<'a> {
    pub t: &'a Theme,
    pub tree: &'a Tree,
    pub row: Row,
    pub index: usize,
    pub is_cursor: bool,
    pub is_open: bool,
    pub is_loading: bool,
}

fn key_text(tree: &Tree, node: NodeId) -> String {
    match tree.key(node) {
        Key::Name(name) => name.to_owned(),
        Key::Index(index) => index.to_string(),
        Key::Root if tree.flavor() == Flavor::Json => "$".to_owned(),
        Key::Root => String::new(),
    }
}

fn badge_text(tree: &Tree, node: NodeId) -> String {
    let count = tree.child_count(node);
    match tree.kind(node) {
        NodeKind::Object => format!("{{{count}}}"),
        NodeKind::Array => format!("[{count}]"),
        NodeKind::Element if count > 0 => count.to_string(),
        NodeKind::Element => String::new(),
        NodeKind::String => "str".to_owned(),
        NodeKind::Number => "num".to_owned(),
        NodeKind::Bool => "bool".to_owned(),
        NodeKind::Null => "null".to_owned(),
    }
}

fn kind_color(c: &Colors, kind: NodeKind) -> Hsla {
    match kind {
        NodeKind::Object | NodeKind::Array => c.kind_archive,
        NodeKind::Element => c.kind_code,
        NodeKind::String => c.kind_app,
        NodeKind::Number => c.kind_code,
        NodeKind::Bool => c.kind_image,
        NodeKind::Null => c.muted,
    }
}

pub(super) fn spoken(tree: &Tree, node: NodeId) -> String {
    [
        key_text(tree, node),
        badge_text(tree, node),
        tree.attrs(node).to_owned(),
        tree.value(node).to_owned(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}

fn inset_ring(color: Hsla) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(1.),
        inset: true,
    }]
}

fn guides(look: &RowLook<'_>) -> impl IntoElement {
    let c = &look.t.colors;
    div()
        .flex()
        .flex_none()
        .h_full()
        .children((0..look.row.depth).map(|_| {
            div().w(px(INDENT)).h_full().flex_none().child(
                div()
                    .ml(px(INDENT / 2.))
                    .h_full()
                    .border_l_1()
                    .border_color(c.line),
            )
        }))
}

fn chevron(look: &RowLook<'_>, view: &WeakEntity<TreeView>) -> impl IntoElement {
    let c = &look.t.colors;
    let has_children = look.tree.child_count(look.row.node) > 0;
    let index = look.index;
    let view = view.clone();
    div()
        .id(("tree-chevron", index))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(INDENT))
        .h_full()
        .when(has_children, |slot| {
            let name = if look.is_open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            };
            let color = if look.is_loading {
                c.accent_text
            } else {
                c.faint
            };
            slot.cursor_pointer()
                .child(icon(name, px(11.), color))
                .on_click(move |_: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    let _ = view.update(cx, |view, cx| view.toggle_row(index, window, cx));
                })
        })
}

fn badge(look: &RowLook<'_>) -> Option<Div> {
    let text = badge_text(look.tree, look.row.node);
    if text.is_empty() {
        return None;
    }
    let t = look.t;
    let color = kind_color(&t.colors, look.tree.kind(look.row.node));
    Some(
        div()
            .flex_none()
            .px(t.spacing.xs)
            .rounded(t.radii.xs)
            .bg(color.opacity(0.12))
            .text_color(color)
            .text_size(t.typography.tiny)
            .font_family(t.typography.mono_font.clone())
            .child(text),
    )
}

fn value_color(c: &Colors, kind: NodeKind) -> Hsla {
    match kind {
        NodeKind::Number => c.kind_code,
        NodeKind::Bool => c.kind_image,
        NodeKind::Null => c.muted,
        NodeKind::Object | NodeKind::Array | NodeKind::Element | NodeKind::String => c.text,
    }
}

fn select_row(
    view: WeakEntity<TreeView>,
    index: usize,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |event, window, cx| {
        let twice = event.click_count() > 1;
        let _ = view.update(cx, |view, cx| {
            if twice {
                view.toggle_row(index, window, cx);
            } else {
                view.click_row(index, window, cx);
            }
        });
    }
}

fn key_view(look: &RowLook<'_>) -> Div {
    let t = look.t;
    let node = look.row.node;
    let color = if matches!(look.tree.key(node), Key::Index(_)) {
        t.colors.muted
    } else {
        t.colors.text
    };
    div()
        .flex_none()
        .max_w(t.spacing.rail_width)
        .truncate()
        .font_family(t.typography.mono_font.clone())
        .text_color(color)
        .when(look.tree.kind(node) == NodeKind::Element, |key| {
            key.font_weight(FontWeight::MEDIUM)
        })
        .child(key_text(look.tree, node))
}

fn detail_view(t: &Theme, text: &str, color: Hsla) -> Div {
    div()
        .truncate()
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small)
        .text_color(color)
        .child(text.to_owned())
}

pub(super) fn tree_row(look: &RowLook<'_>, view: &WeakEntity<TreeView>) -> AnyElement {
    let t = look.t;
    let c = &t.colors;
    let node = look.row.node;
    let tree = look.tree;
    let attrs = tree.attrs(node);
    let value = tree.value(node);
    div()
        .id(("tree-row", look.index))
        .aria_label(spoken(tree, node))
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .w_full()
        .h_full()
        .pl(t.spacing.md)
        .pr(t.spacing.xl)
        .overflow_hidden()
        .whitespace_nowrap()
        .when(look.is_cursor, |row| {
            row.bg(c.selection).shadow(inset_ring(c.accent))
        })
        .on_click(select_row(view.clone(), look.index))
        .child(guides(look))
        .child(chevron(look, view))
        .child(key_view(look))
        .children(badge(look))
        .when(!attrs.is_empty(), |row| {
            row.child(
                detail_view(t, attrs, c.muted)
                    .flex_none()
                    .max_w(t.spacing.rail_width * 1.5),
            )
        })
        .when(!value.is_empty(), |row| {
            row.child(detail_view(t, value, value_color(c, tree.kind(node))).min_w_0())
        })
        .into_any_element()
}
