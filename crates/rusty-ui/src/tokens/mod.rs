mod contrast;
mod density;
mod design_b;
mod kit;
mod marks;
mod memory;
mod motion;
mod palette;
mod shadows;
mod surfaces;
mod syntax;

#[cfg(test)]
mod tests;

use std::sync::LazyLock;

use gpui_kit::{App, Global, Hsla, Pixels, SharedString};

pub use contrast::{contrast_ratio, relative_luminance};
pub use density::{Density, Rows};
pub use kit::{ThemeLook, apply, apply_look, apply_with};
pub use marks::MarkColors;
pub use memory::{MemColors, MemKind};
pub use motion::Motion;
pub use palette::ColorTheme;
pub use shadows::Shadows;
pub use surfaces::Surfaces;
pub use syntax::{SyntaxTheme, highlight_theme};

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
    pub kind_pdf: Hsla,
    pub kind_doc: Hsla,
    pub kind_sheet: Hsla,
    pub kind_slides: Hsla,
    pub kind_audio: Hsla,
    pub kind_video: Hsla,
    pub kind_auto: Hsla,
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
    pub color_theme: ColorTheme,
    pub colors: Colors,
    pub typography: Typography,
    pub spacing: Spacing,
    pub radii: Radii,
    pub marks: MarkColors,
    pub mem: MemColors,
    pub surfaces: Surfaces,
    pub shadows: Shadows,
    pub motion: Motion,
    pub density: Density,
    pub rows: Rows,
    pub syntax: SyntaxTheme,
}

impl Global for Theme {}

impl Theme {
    pub fn new(mode: Mode) -> Self {
        Self::with_density(mode, Density::Normal)
    }

    pub fn with_density(mode: Mode, density: Density) -> Self {
        Self::with_look(
            mode,
            ThemeLook {
                density,
                ..ThemeLook::default()
            },
        )
    }

    pub fn with_look(mode: Mode, look: ThemeLook) -> Self {
        let tokens = palette::tokens(look.color_theme, mode);
        let colors = palette::colors(&tokens);
        let surfaces = surfaces::surfaces(mode, &colors, &tokens);
        let spacing = design_b::spacing();
        Self {
            mode,
            color_theme: look.color_theme,
            colors,
            typography: design_b::typography(),
            rows: density::rows(look.density, &spacing),
            density: look.density,
            spacing,
            radii: design_b::radii(),
            marks: marks::mark_colors(mode),
            mem: memory::mem_colors(mode),
            shadows: shadows::shadows(mode, &surfaces),
            surfaces,
            motion: motion::motion(),
            syntax: look.syntax,
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
    fn base_entries(&self) -> [(&'static str, Hsla); 23] {
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
            ..
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
            ("terminal_background", self.terminal_background),
            ("terminal_text", self.terminal_text),
            ("terminal_muted", self.terminal_muted),
        ]
    }

    fn kind_entries(&self) -> [(&'static str, Hsla); 7] {
        [
            ("kind_pdf", self.kind_pdf),
            ("kind_doc", self.kind_doc),
            ("kind_sheet", self.kind_sheet),
            ("kind_slides", self.kind_slides),
            ("kind_audio", self.kind_audio),
            ("kind_video", self.kind_video),
            ("kind_auto", self.kind_auto),
        ]
    }

    pub fn entries(&self) -> Vec<(&'static str, Hsla)> {
        self.base_entries()
            .into_iter()
            .chain(self.kind_entries())
            .collect()
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
