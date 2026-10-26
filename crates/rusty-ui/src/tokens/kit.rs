use std::sync::Arc;

use gpui_kit::App;
use gpui_kit::component::theme::{Theme as KitTheme, ThemeColor, ThemeMode};

use super::{ColorTheme, Colors, Density, Mode, SyntaxTheme, Theme, highlight_theme};

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

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ThemeLook {
    pub density: Density,
    pub syntax: SyntaxTheme,
    pub color_theme: ColorTheme,
}

impl ThemeLook {
    fn kept(cx: &App) -> Self {
        cx.try_global::<Theme>()
            .map(|theme| Self {
                density: theme.density,
                syntax: theme.syntax,
                color_theme: theme.color_theme,
            })
            .unwrap_or_default()
    }
}

pub fn apply(mode: Mode, cx: &mut App) {
    apply_look(mode, ThemeLook::kept(cx), cx);
}

pub fn apply_with(mode: Mode, density: Density, cx: &mut App) {
    let kept = ThemeLook::kept(cx);
    apply_look(mode, ThemeLook { density, ..kept }, cx);
}

pub fn apply_look(mode: Mode, look: ThemeLook, cx: &mut App) {
    let syntax = look.syntax;
    let theme = Theme::with_look(mode, look);
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
        let highlight = highlight_theme(syntax, mode, &colors, &kit.highlight_theme);
        kit.highlight_theme = Arc::new(highlight);
    });
}

impl Theme {
    pub fn scale(&mut self, percent: u16) {
        let factor = f32::from(percent) / 100.;
        let typography = &mut self.typography;
        typography.body *= factor;
        typography.small *= factor;
        typography.tiny *= factor;
        typography.key *= factor;
        typography.line_height *= factor;
        let spacing = &mut self.spacing;
        spacing.xxs *= factor;
        spacing.xs *= factor;
        spacing.sm *= factor;
        spacing.md *= factor;
        spacing.lg *= factor;
        spacing.xl *= factor;
        spacing.xxl *= factor;
        spacing.icon *= factor;
        spacing.row_height *= factor;
        spacing.tab_height *= factor;
        spacing.path_height *= factor;
        spacing.header_height *= factor;
        spacing.status_height *= factor;
        spacing.fkey_height *= factor;
        spacing.rail_width *= factor;
        spacing.rail_item_height *= factor;
        spacing.drawer_height *= factor;
        spacing.size_column *= factor;
        spacing.date_column *= factor;
        let radii = &mut self.radii;
        radii.xs *= factor;
        radii.sm *= factor;
        radii.md *= factor;
        radii.lg *= factor;
        radii.card *= factor;
        radii.pill *= factor;
        radii.full *= factor;
        radii.keycap *= factor;
        radii.sheet *= factor;
        radii.job_card *= factor;
        let rows = &mut self.rows;
        rows.height *= factor;
        rows.inset *= factor;
    }
}
