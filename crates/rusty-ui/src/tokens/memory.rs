use gpui_kit::Hsla;
use gpui_kit::component::theme::oklch;

use super::Mode;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MemKind {
    pub strong: Hsla,
    pub tint: Hsla,
    pub text: Hsla,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MemColors {
    pub code: MemKind,
    pub rodata: MemKind,
    pub data: MemKind,
    pub bss: MemKind,
    pub other: MemKind,
    pub free: Hsla,
    pub free_line: Hsla,
}

impl MemColors {
    pub fn kinds(&self) -> [(&'static str, MemKind); 5] {
        [
            ("code", self.code),
            ("rodata", self.rodata),
            ("data", self.data),
            ("bss", self.bss),
            ("other", self.other),
        ]
    }
}

fn kind(hue: f32, steps: [(f32, f32); 3]) -> MemKind {
    let [strong, tint, text] = steps.map(|(lightness, chroma)| oklch(lightness, chroma, hue));
    MemKind { strong, tint, text }
}

pub(super) fn mem_colors(mode: Mode) -> MemColors {
    match mode {
        Mode::Light => MemColors {
            code: kind(40., [(0.60, 0.116), (0.94, 0.018), (0.46, 0.108)]),
            rodata: kind(82., [(0.60, 0.106), (0.94, 0.017), (0.46, 0.094)]),
            data: kind(210., [(0.60, 0.086), (0.94, 0.013), (0.46, 0.078)]),
            bss: kind(238., [(0.60, 0.09), (0.94, 0.013), (0.46, 0.08)]),
            other: kind(60., [(0.60, 0.025), (0.94, 0.006), (0.46, 0.025)]),
            free: oklch(0.965, 0.004, 260.),
            free_line: oklch(0.6, 0.016, 260.),
        },
        Mode::Dark => MemColors {
            code: kind(40., [(0.68, 0.104), (0.30, 0.04), (0.84, 0.052)]),
            rodata: kind(82., [(0.68, 0.094), (0.30, 0.036), (0.84, 0.05)]),
            data: kind(210., [(0.68, 0.078), (0.30, 0.03), (0.84, 0.04)]),
            bss: kind(238., [(0.68, 0.08), (0.30, 0.03), (0.84, 0.04)]),
            other: kind(60., [(0.68, 0.022), (0.30, 0.012), (0.84, 0.014)]),
            free: oklch(0.21, 0.01, 260.),
            free_line: oklch(0.58, 0.014, 260.),
        },
    }
}
