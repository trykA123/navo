use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::FluentBuilder as _;
use crate::tokens::{self, Theme};

#[derive(IntoElement)]
pub struct FKeyBar {
    id: ElementId,
    keys: Vec<(SharedString, SharedString)>,
    is_sized_to_keys: bool,
}

impl FKeyBar {
    pub fn new(
        id: impl Into<ElementId>,
        keys: impl IntoIterator<Item = (impl Into<SharedString>, impl Into<SharedString>)>,
    ) -> Self {
        Self {
            id: id.into(),
            keys: keys
                .into_iter()
                .map(|(key, label)| (key.into(), label.into()))
                .collect(),
            is_sized_to_keys: false,
        }
    }

    #[must_use]
    pub fn sized_to_keys(mut self) -> Self {
        self.is_sized_to_keys = true;
        self
    }
}

struct Cell {
    index: usize,
    key: SharedString,
    label: SharedString,
    is_sized: bool,
}

fn cell(t: &Theme, cell: Cell) -> impl IntoElement {
    let Cell {
        index,
        key,
        label,
        is_sized,
    } = cell;
    div()
        .id(("fkey", index))
        .role(Role::Label)
        .aria_label(format!("{key} {label}"))
        .test_support()
        .flex()
        .items_center()
        .justify_center()
        .gap(t.spacing.md)
        .map(|cell| {
            if is_sized {
                cell.flex_auto()
            } else {
                cell.flex_1()
            }
        })
        .min_w_0()
        .text_size(t.typography.small)
        .font_weight(FontWeight::MEDIUM)
        .bg(t.colors.rail)
        .child(
            div()
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.key)
                .font_weight(FontWeight::BOLD)
                .text_color(t.colors.accent_text)
                .child(key),
        )
        .child(div().truncate().child(label))
}

impl RenderOnce for FKeyBar {
    fn render(self, _: &mut Window, cx: &mut gpui_kit::App) -> impl IntoElement {
        let t = tokens::current(cx);
        div()
            .id(self.id)
            .role(Role::Group)
            .aria_label("Function keys")
            .test_support()
            .flex()
            .flex_none()
            .h(t.spacing.fkey_height)
            .border_t_1()
            .border_color(t.colors.line)
            .gap(px(1.))
            .bg(t.colors.line)
            .text_color(t.colors.text)
            .children(
                self.keys
                    .into_iter()
                    .enumerate()
                    .map(|(index, (key, label))| {
                        let is_sized = self.is_sized_to_keys;
                        cell(
                            t,
                            Cell {
                                index,
                                key,
                                label,
                                is_sized,
                            },
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::AppContext as _;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{Context, IntoElement, Render, Role, TestAppContext, Window};

    use super::FKeyBar;
    use crate::test_window::open;

    struct Keys;

    impl Render for Keys {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            FKeyBar::new("fkeys", [("F3", "View"), ("F5", "Copy"), ("F8", "Delete")])
        }
    }

    #[gpui_kit::test]
    fn the_bar_shows_each_key_with_its_label(cx: &mut TestAppContext) {
        let (window, _) = open(cx, |_, _| Keys);
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("fkeys").role(), Some(Role::Group));
            let keys = window.within("fkeys");
            let labels: Vec<Option<String>> = (0..3usize)
                .map(|index| keys.find(("fkey", index)).label().map(str::to_owned))
                .collect();
            assert_eq!(
                labels,
                [Some("F3 View"), Some("F5 Copy"), Some("F8 Delete")].map(|l| l.map(str::to_owned))
            );
            assert!(keys.try_find(("fkey", 3usize)).is_none());
        })
        .expect("window");
    }
}
