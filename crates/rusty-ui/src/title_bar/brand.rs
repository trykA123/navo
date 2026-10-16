use std::sync::{Arc, LazyLock};

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    FontWeight, Image, ImageFormat, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, div, img, px,
};

use super::AppButtonHandler;
use crate::primitives::{IconName, icon as chevron};
use crate::tokens::Theme;

macro_rules! brand_svg {
    ($file:literal) => {
        LazyLock::new(|| {
            let bytes = include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/brand/",
                $file
            ));
            Arc::new(Image::from_bytes(ImageFormat::Svg, bytes.to_vec()))
        })
    };
}

static ICON_1X: LazyLock<Arc<Image>> = brand_svg!("ferrite-24.svg");
static ICON_2X: LazyLock<Arc<Image>> = brand_svg!("ferrite-48.svg");

const ICON_SIZE: f32 = 24.;

pub(crate) fn icon_for(scale_factor: f32) -> Arc<Image> {
    if scale_factor > 1. {
        ICON_2X.clone()
    } else {
        ICON_1X.clone()
    }
}

pub(super) fn brand(
    t: &Theme,
    title: SharedString,
    scale_factor: f32,
    button: Option<(bool, AppButtonHandler)>,
) -> impl IntoElement {
    let icon = div()
        .id("title-bar-icon")
        .test_support()
        .flex_none()
        .size(px(ICON_SIZE))
        .child(img(icon_for(scale_factor)).size_full());
    let role = if button.is_some() {
        Role::Button
    } else {
        Role::Label
    };
    div()
        .id("title-bar-title")
        .role(role)
        .aria_label(title.clone())
        .test_support()
        .when_some(button, |brand, (is_open, handler)| {
            brand
                .aria_expanded(is_open)
                .occlude()
                .cursor_pointer()
                .hover(|brand| brand.bg(t.colors.hover))
                .when(is_open, |brand| brand.bg(t.colors.hover))
                .on_click(move |event, window, cx| handler(event, window, cx))
        })
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .flex_none()
        .py(t.spacing.xs)
        .pl(t.spacing.xs)
        .pr(t.spacing.md)
        .rounded(t.radii.card)
        .font_weight(FontWeight::SEMIBOLD)
        .child(icon)
        .child(title)
        .child(chevron(
            IconName::ChevronDown,
            t.spacing.icon - px(1.),
            t.colors.muted,
        ))
}
