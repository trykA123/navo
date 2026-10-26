mod dark;
mod light;

use gpui_kit::Hsla;

use super::design_b::ok;
use super::{Colors, Mode};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ColorTheme {
    #[default]
    OpenSea,
    RedBill,
    BlackCap,
    FineMesh,
    Grain,
    InkAndAmber,
}

impl ColorTheme {
    pub const ALL: [Self; 6] = [
        Self::OpenSea,
        Self::RedBill,
        Self::BlackCap,
        Self::FineMesh,
        Self::Grain,
        Self::InkAndAmber,
    ];

    pub fn toml(self) -> &'static str {
        match self {
            Self::OpenSea => "open-sea",
            Self::RedBill => "red-bill",
            Self::BlackCap => "black-cap",
            Self::FineMesh => "fine-mesh",
            Self::Grain => "grain",
            Self::InkAndAmber => "ink-and-amber",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Lch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

pub(super) const fn lch(l: f32, c: f32, h: f32) -> Lch {
    Lch { l, c, h }
}

impl Lch {
    pub fn color(self) -> Hsla {
        ok(self.l, self.c, self.h)
    }

    pub fn mix(self, toward: Self, amount: f32) -> Self {
        let step = |from: f32, to: f32| from + (to - from) * amount;
        lch(step(self.l, toward.l), step(self.c, toward.c), toward.h)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Tokens {
    pub bg: Lch,
    pub surface: Lch,
    pub raised: Lch,
    pub border: Lch,
    pub selected: Lch,
    pub text: Lch,
    pub muted: Lch,
    pub border_strong: Lch,
    pub accent: Lch,
    pub accent_text: Lch,
    pub danger: Lch,
    pub success: Lch,
    pub warning: Lch,
    pub info: Lch,
}

const KIND_HUES: [f32; 11] = [
    262., 340., 45., 150., 25., 235., 165., 75., 300., 195., 120.,
];

pub(super) fn tokens(theme: ColorTheme, mode: Mode) -> Tokens {
    match mode {
        Mode::Light => light::tokens(theme),
        Mode::Dark => dark::tokens(theme),
    }
}

pub(super) fn colors(t: &Tokens) -> Colors {
    let kind = |at: usize| lch(t.info.l, t.info.c, KIND_HUES[at]).color();
    Colors {
        background: t.surface.color(),
        surface: t.raised.color(),
        rail: t.bg.color(),
        zebra: t.surface.mix(t.bg, 0.5).color(),
        line: t.border.color(),
        text: t.text.color(),
        muted: t.muted.color(),
        faint: t.border_strong.color(),
        hover: t.surface.mix(t.selected, 0.5).color(),
        selection: t.selected.color(),
        accent: t.accent.color(),
        accent_text: t.accent_text.color(),
        mark: lch(t.accent_text.l, t.danger.c, t.danger.h).color(),
        folder: t.warning.color(),
        kind_code: kind(0),
        kind_image: kind(1),
        kind_archive: kind(2),
        kind_app: kind(3),
        kind_pdf: kind(4),
        kind_doc: kind(5),
        kind_sheet: kind(6),
        kind_slides: kind(7),
        kind_audio: kind(8),
        kind_video: kind(9),
        kind_auto: kind(10),
        ok: t.success.color(),
        warn: t.warning.color(),
        terminal_background: t.bg.color(),
        terminal_text: t.text.color(),
        terminal_muted: t.muted.color(),
    }
}
