use super::*;
use crate::tokens::{Mode, SystemPalette, contrast_theme};
use gpui_kit::{black, rgb, white};

#[test]
fn highlighted_tab_uses_the_system_selection_foreground() {
    let palette = SystemPalette {
        window: black(),
        window_text: white(),
        highlight: white(),
        highlight_text: black(),
        button_face: rgb(0x222222).into(),
        button_text: white(),
        gray_text: rgb(0x767676).into(),
        hot_light: rgb(0xffff00).into(),
    };
    let theme = contrast_theme(&Theme::new(Mode::Dark), &palette);
    for (is_selected, is_highlighted, expected) in [
        (true, true, palette.highlight_text),
        (true, false, palette.window_text),
        (false, true, palette.gray_text),
    ] {
        let mut tab = render_tab(
            Tab::new("Folder"),
            TabProps {
                theme: &theme,
                index: 0,
                count: 1,
                is_selected,
                is_highlighted,
                min_width: px(88.),
                on_select: None,
                on_close: None,
                on_bounds: None,
                on_menu: None,
                landing: Landing::default(),
            },
        );
        assert_eq!(tab.style().text.color, Some(expected));
    }
}
