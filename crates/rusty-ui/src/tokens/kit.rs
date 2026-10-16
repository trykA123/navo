use gpui_kit::App;
use gpui_kit::component::theme::{Theme as KitTheme, ThemeColor, ThemeMode};

use super::{Colors, Density, Mode, Theme};

fn map_colors(c: &Colors, k: &mut ThemeColor) {
    k.background = c.background;
    k.foreground = c.text;
    k.border = c.line;
    k.input = c.line;
    k.ring = c.accent;
    k.muted = c.hover;
    k.muted_foreground = c.muted;
    k.accent = c.hover;
    k.accent_foreground = c.text;
    k.primary = c.accent;
    k.primary_hover = c.accent_text;
    k.primary_active = c.accent_text;
    k.primary_foreground = c.background;
    k.button_primary = c.accent;
    k.button_primary_hover = c.accent_text;
    k.button_primary_active = c.accent_text;
    k.button_primary_foreground = c.background;
    k.danger = c.mark;
    k.danger_hover = c.mark;
    k.danger_active = c.mark;
    k.danger_foreground = c.background;
    k.button_danger = c.mark;
    k.button_danger_hover = c.mark;
    k.button_danger_active = c.mark;
    k.button_danger_foreground = c.background;
    k.button = c.surface;
    k.button_hover = c.hover;
    k.button_active = c.selection;
    k.button_foreground = c.text;
    k.secondary = c.surface;
    k.secondary_hover = c.hover;
    k.secondary_active = c.selection;
    k.secondary_foreground = c.text;
    k.progress_bar = c.accent;
    k.scrollbar = c.surface;
    k.scrollbar_thumb = c.line;
    k.scrollbar_thumb_hover = c.faint;
    k.popover = c.surface;
    k.popover_foreground = c.text;
    k.tab_bar = c.rail;
    k.tab = c.rail;
    k.tab_active = c.surface;
    k.tab_foreground = c.muted;
    k.tab_active_foreground = c.text;
}

pub fn apply(mode: Mode, cx: &mut App) {
    let density = cx.try_global::<Theme>().map(|theme| theme.density);
    apply_with(mode, density.unwrap_or_default(), cx);
}

pub fn apply_with(mode: Mode, density: Density, cx: &mut App) {
    let theme = Theme::with_density(mode, density);
    cx.set_global(theme.clone());
    let kit_mode = if mode.is_dark() {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    KitTheme::change(kit_mode, None, cx);
    let colors = theme.colors;
    let typography = theme.typography.clone();
    let radius = theme.radii.md;
    KitTheme::update(cx, move |kit| {
        kit.font_family = typography.ui_font;
        kit.mono_font_family = typography.mono_font;
        kit.radius = radius;
        kit.shadow = false;
        map_colors(&colors, &mut kit.colors);
    });
}
