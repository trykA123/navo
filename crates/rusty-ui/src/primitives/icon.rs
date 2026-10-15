use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::{Hsla, IntoElement, Pixels, Styled};

pub fn icon(name: IconName, size: Pixels, color: Hsla) -> impl IntoElement {
    Icon::new(name).size(size).flex_none().text_color(color)
}
