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

static ICON_1X: LazyLock<Arc<Image>> = brand_svg!("navo-24.svg");
static ICON_2X: LazyLock<Arc<Image>> = brand_svg!("navo-48.svg");

const ICON_SIZE: f32 = 24.;

pub(crate) fn icon_for(scale_factor: f32) -> Arc<Image> {
    if scale_factor > 1. {
        ICON_2X.clone()
    } else {
        ICON_1X.clone()
    }
}

/// The mark left of the title, one image per scale band; `at_2x` above 100 % scale.
#[derive(Clone)]
pub struct BrandMark {
    /// The image drawn at 100 % scale and below.
    pub at_1x: Arc<Image>,
    /// The image drawn above 100 % scale.
    pub at_2x: Arc<Image>,
}

/// What the brand end of the title bar shows: the app `title`, the `mark` before it and the
/// `menu_arrow` chevron after it.
pub(super) struct Brand {
    pub(super) title: SharedString,
    pub(super) mark: Option<BrandMark>,
    pub(super) menu_arrow: bool,
}

/// The mark image for `scale_factor`: the caller's `mark` when set, Navo's own logo otherwise.
fn image_of(mark: Option<BrandMark>, scale_factor: f32) -> Arc<Image> {
    mark.map_or_else(
        || icon_for(scale_factor),
        |mark| {
            if scale_factor > 1. {
                mark.at_2x
            } else {
                mark.at_1x
            }
        },
    )
}

/// The fixed `ICON_SIZE` square the mark draws in; `image` fills it.
fn mark_icon(image: Arc<Image>) -> impl IntoElement {
    div()
        .id("title-bar-icon")
        .test_support()
        .flex_none()
        .size(px(ICON_SIZE))
        .child(img(image).size_full())
}

/// The chevron that hints the title opens the app menu; a bare brand leaves it out.
fn menu_arrow(t: &Theme) -> impl IntoElement {
    div()
        .id("title-bar-arrow")
        .test_support()
        .flex_none()
        .child(chevron(
            IconName::ChevronDown,
            t.spacing.icon - px(1.),
            t.colors.muted,
        ))
}

/// The brand end of the title bar: `brand`'s mark, `brand.title` and, when the brand asks, the
/// menu chevron; `button` turns the whole end into the app-menu button.
pub(super) fn brand(
    t: &Theme,
    brand: Brand,
    scale_factor: f32,
    button: Option<(bool, AppButtonHandler)>,
) -> impl IntoElement {
    let icon = mark_icon(image_of(brand.mark, scale_factor));
    let role = if button.is_some() {
        Role::Button
    } else {
        Role::Label
    };
    div()
        .id("title-bar-title")
        .role(role)
        .aria_label(brand.title.clone())
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
        .font_family(t.typography.brand_font.clone())
        .font_weight(FontWeight::SEMIBOLD)
        .child(icon)
        .child(brand.title)
        .when(brand.menu_arrow, |brand| brand.child(menu_arrow(t)))
}
