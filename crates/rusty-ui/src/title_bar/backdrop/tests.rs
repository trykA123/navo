use gpui_kit::WindowBackgroundAppearance;

use super::{Backdrop, GradientLayer, backdrop_for};

const OPAQUE: Backdrop = Backdrop {
    window: WindowBackgroundAppearance::Opaque,
    gradient: GradientLayer::Root,
};

const MICA: Backdrop = Backdrop {
    window: WindowBackgroundAppearance::MicaBackdrop,
    gradient: GradientLayer::BelowTitleBar,
};

#[test]
fn windows_10_gets_an_opaque_window_with_the_gradient_on_the_root() {
    assert_eq!(backdrop_for(Some(19045)), OPAQUE);
}

#[test]
fn the_first_windows_11_build_without_the_backdrop_api_stays_opaque() {
    assert_eq!(backdrop_for(Some(22000)), OPAQUE);
}

#[test]
fn windows_11_builds_with_the_backdrop_api_get_mica() {
    assert_eq!(backdrop_for(Some(22621)), MICA);
    assert_eq!(backdrop_for(Some(26100)), MICA);
}

#[test]
fn other_systems_get_an_opaque_window_with_the_gradient_on_the_root() {
    assert_eq!(backdrop_for(None), OPAQUE);
}
