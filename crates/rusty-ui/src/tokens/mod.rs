mod contrast;
mod design_b;
mod kit;
mod marks;
mod motion;
mod shadows;
mod surfaces;

#[cfg(test)]
mod tests;

use std::sync::LazyLock;

use gpui_kit::{App, Global, Hsla, Pixels, SharedString};

pub use contrast::{contrast_ratio, relative_luminance};
pub use kit::apply;
pub use marks::MarkColors;
pub use motion::Motion;
pub use shadows::Shadows;
pub use surfaces::Surfaces;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Light,
    Dark,
}

impl Mode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn resolve(self, system: Mode) -> Mode {
        match self {
            Self::System => system,
            Self::Light => Mode::Light,
            Self::Dark => Mode::Dark,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Colors {
    pub background: Hsla,
    pub surface: Hsla,
    pub rail: Hsla,
    pub zebra: Hsla,
    pub line: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
    pub hover: Hsla,
    pub selection: Hsla,
    pub accent: Hsla,
    pub accent_text: Hsla,
    pub mark: Hsla,
    pub folder: Hsla,
    pub kind_code: Hsla,
    pub kind_image: Hsla,
    pub kind_archive: Hsla,
    pub kind_app: Hsla,
    pub ok: Hsla,
    pub warn: Hsla,
    pub terminal_background: Hsla,
    pub terminal_text: Hsla,
    pub terminal_muted: Hsla,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Typography {
    pub ui_font: SharedString,
    pub numbers_font: SharedString,
    pub mono_font: SharedString,
    pub body: Pixels,
    pub small: Pixels,
    pub tiny: Pixels,
    pub key: Pixels,
    pub line_height: Pixels,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Spacing {
    pub xxs: Pixels,
    pub xs: Pixels,
    pub sm: Pixels,
    pub md: Pixels,
    pub lg: Pixels,
    pub xl: Pixels,
    pub xxl: Pixels,
    pub icon: Pixels,
    pub row_height: Pixels,
    pub tab_height: Pixels,
    pub path_height: Pixels,
    pub header_height: Pixels,
    pub status_height: Pixels,
    pub fkey_height: Pixels,
    pub rail_width: Pixels,
    pub rail_item_height: Pixels,
    pub drawer_height: Pixels,
    pub size_column: Pixels,
    pub date_column: Pixels,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Radii {
    pub xs: Pixels,
    pub sm: Pixels,
    pub md: Pixels,
    pub lg: Pixels,
    pub card: Pixels,
    pub pill: Pixels,
    pub full: Pixels,
    pub keycap: Pixels,
    pub sheet: Pixels,
    pub job_card: Pixels,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Theme {
    pub mode: Mode,
    pub colors: Colors,
    pub typography: Typography,
    pub spacing: Spacing,
    pub radii: Radii,
    pub marks: MarkColors,
    pub surfaces: Surfaces,
    pub shadows: Shadows,
    pub motion: Motion,
}

impl Global for Theme {}

impl Theme {
    pub fn new(mode: Mode) -> Self {
        let colors = design_b::colors(mode);
        let surfaces = surfaces::surfaces(mode, &colors);
        Self {
            mode,
            colors,
            typography: design_b::typography(),
            spacing: design_b::spacing(),
            radii: design_b::radii(),
            marks: marks::mark_colors(mode),
            shadows: shadows::shadows(mode, &surfaces),
            surfaces,
            motion: motion::motion(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ColorPair {
    pub foreground: &'static str,
    pub background: &'static str,
    pub ratio: f32,
}

const TEXT_TOKENS: [&str; 4] = ["text", "muted", "accent_text", "mark"];
const TEXT_SURFACES: [&str; 6] = [
    "background",
    "surface",
    "rail",
    "zebra",
    "hover",
    "selection",
];
const FILLED_PAIRS: [(&str, &str); 4] = [
    ("background", "accent"),
    ("background", "accent_text"),
    ("terminal_text", "terminal_background"),
    ("terminal_muted", "terminal_background"),
];

impl Colors {
    pub fn entries(&self) -> [(&'static str, Hsla); 23] {
        let Self {
            background,
            surface,
            rail,
            zebra,
            line,
            text,
            muted,
            faint,
            hover,
            selection,
            accent,
            accent_text,
            mark,
            folder,
            kind_code,
            kind_image,
            kind_archive,
            kind_app,
            ok,
            warn,
            terminal_background,
            terminal_text,
            terminal_muted,
        } = *self;
        [
            ("background", background),
            ("surface", surface),
            ("rail", rail),
            ("zebra", zebra),
            ("line", line),
            ("text", text),
            ("muted", muted),
            ("faint", faint),
            ("hover", hover),
            ("selection", selection),
            ("accent", accent),
            ("accent_text", accent_text),
            ("mark", mark),
            ("folder", folder),
            ("kind_code", kind_code),
            ("kind_image", kind_image),
            ("kind_archive", kind_archive),
            ("kind_app", kind_app),
            ("ok", ok),
            ("warn", warn),
            ("terminal_background", terminal_background),
            ("terminal_text", terminal_text),
            ("terminal_muted", terminal_muted),
        ]
    }

    pub fn get(&self, name: &str) -> Option<Hsla> {
        self.entries()
            .into_iter()
            .find_map(|(entry, color)| (entry == name).then_some(color))
    }

    pub fn text_pairs(&self) -> Vec<ColorPair> {
        let on_surfaces = TEXT_TOKENS
            .iter()
            .flat_map(|fg| TEXT_SURFACES.iter().map(move |bg| (*fg, *bg)));
        on_surfaces
            .chain(FILLED_PAIRS)
            .filter_map(|(fg, bg)| {
                let ratio = contrast_ratio(self.get(fg)?, self.get(bg)?);
                Some(ColorPair {
                    foreground: fg,
                    background: bg,
                    ratio,
                })
            })
            .collect()
    }
}

static FALLBACK: LazyLock<Theme> = LazyLock::new(|| Theme::new(Mode::Light));

pub fn current(cx: &App) -> &Theme {
    cx.try_global::<Theme>().unwrap_or(&FALLBACK)
}
