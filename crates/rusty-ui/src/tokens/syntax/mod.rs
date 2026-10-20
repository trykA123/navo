mod fallback;
mod iron;
mod mocha;

#[cfg(test)]
mod tests;

use gpui_kit::component::highlighter::{HighlightTheme, SyntaxColors};
use gpui_kit::component::theme::ThemeMode;
use gpui_kit::{Hsla, Rgba};

use super::{Colors, Mode};
use fallback::FALLBACKS;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SyntaxTheme {
    #[default]
    OxidisedIron,
    Mocha,
}

impl SyntaxTheme {
    pub const ALL: [Self; 2] = [Self::OxidisedIron, Self::Mocha];

    pub fn toml(self) -> &'static str {
        match self {
            Self::OxidisedIron => "oxidised-iron",
            Self::Mocha => "mocha",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Style {
    Plain,
    Italic,
    Bold,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Role {
    pub name: &'static str,
    pub color: Hsla,
    pub style: Style,
}

pub(super) fn role(name: &'static str, color: Hsla, style: Style) -> Role {
    Role { name, color, style }
}

fn roles(syntax: SyntaxTheme, mode: Mode, colors: &Colors) -> Vec<Role> {
    let named = match (syntax, mode) {
        (SyntaxTheme::OxidisedIron, Mode::Light) => iron::light(colors),
        (SyntaxTheme::OxidisedIron, Mode::Dark) => iron::dark(colors),
        (SyntaxTheme::Mocha, Mode::Light) => mocha::light(colors),
        (SyntaxTheme::Mocha, Mode::Dark) => mocha::dark(colors),
    };
    let fallen = FALLBACKS.iter().filter_map(|fallback| {
        let from = named.iter().find(|role| role.name == fallback.from)?;
        Some(Role {
            name: fallback.name,
            color: from.color,
            style: fallback.style.unwrap_or(from.style),
        })
    });
    named.iter().copied().chain(fallen).collect()
}

fn hex(color: Hsla) -> String {
    let rgb = Rgba::from(color);
    let byte = |channel: f32| (channel * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(rgb.r), byte(rgb.g), byte(rgb.b))
}

fn entry(role: &Role) -> toml::Value {
    let mut table = toml::Table::new();
    table.insert("color".into(), hex(role.color).into());
    match role.style {
        Style::Plain => {}
        Style::Italic => {
            table.insert("font_style".into(), "italic".into());
        }
        Style::Bold => {
            table.insert("font_weight".into(), 700_i64.into());
        }
    }
    toml::Value::Table(table)
}

fn key(name: &str) -> String {
    if name == "comment.doc" {
        "comment_doc".to_owned()
    } else {
        name.to_owned()
    }
}

fn syntax_colors(roles: &[Role]) -> Option<SyntaxColors> {
    let table: toml::Table = roles
        .iter()
        .map(|role| (key(role.name), entry(role)))
        .collect();
    toml::Value::Table(table).try_into().ok()
}

pub fn highlight_theme(
    syntax: SyntaxTheme,
    mode: Mode,
    colors: &Colors,
    base: &HighlightTheme,
) -> HighlightTheme {
    let mut theme = base.clone();
    theme.name = syntax.toml().to_owned();
    theme.appearance = if mode.is_dark() {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    if let Some(syntax) = syntax_colors(&roles(syntax, mode, colors)) {
        theme.style.syntax = syntax;
    }
    let style = &mut theme.style;
    style.editor_background = Some(colors.background);
    style.editor_foreground = Some(colors.text);
    style.editor_active_line = Some(colors.zebra);
    style.editor_line_number = Some(colors.faint);
    style.editor_active_line_number = Some(colors.text);
    theme
}
