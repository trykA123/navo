//! Tests of the theme follow (NS19): the appearance mapping and the shell's tokens on open.

use gpui_kit::{AppContext as _, TestAppContext, WindowAppearance};
use sidewise_kit::tokens::{self, Mode};

use crate::shell::Shell;
use crate::test_window::open;
use crate::theme::mode_of;

/// The four appearances map to the dark and light modes.
#[test]
fn mode_of_maps_the_four_appearances() {
    assert_eq!(mode_of(WindowAppearance::Dark), Mode::Dark);
    assert_eq!(mode_of(WindowAppearance::VibrantDark), Mode::Dark);
    assert_eq!(mode_of(WindowAppearance::Light), Mode::Light);
    assert_eq!(mode_of(WindowAppearance::VibrantLight), Mode::Light);
}

/// A shell made while the tokens are Dark applies the mode of the window's appearance.
#[gpui_kit::test]
fn a_new_shell_applies_the_window_appearance(cx: &mut TestAppContext) {
    let (window, _view) = open(cx, Shell::new);
    cx.update(|cx| tokens::apply(Mode::Dark, cx));
    cx.update_window(window, |_, window, cx| {
        let expected = mode_of(window.appearance());
        let _second = cx.new(|cx| Shell::new(window, cx));
        assert_eq!(tokens::current(cx).mode, expected);
    })
    .expect("window");
}
