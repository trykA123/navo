use gpui_kit::Hsla;
use gpui_kit::component::theme::oklch;

use super::Mode;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DiffTone {
    pub text: Hsla,
    pub mark: Hsla,
    pub fill: Hsla,
    pub wash: Hsla,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DiffColors {
    pub delete: DiffTone,
    pub add: DiffTone,
    pub change: DiffTone,
    pub moved: DiffTone,
    pub conflict: DiffTone,
    pub auto_merged: DiffTone,
    pub gap_hatch: Hsla,
}

impl DiffColors {
    pub fn tones(&self) -> [(&'static str, DiffTone); 6] {
        [
            ("delete", self.delete),
            ("add", self.add),
            ("change", self.change),
            ("moved", self.moved),
            ("conflict", self.conflict),
            ("auto_merged", self.auto_merged),
        ]
    }
}

type Step = (f32, f32);

fn tone(hue: f32, [text, mark, fill, wash]: [Step; 4]) -> DiffTone {
    let at = |(lightness, chroma): Step| oklch(lightness, chroma, hue);
    DiffTone {
        text: at(text),
        mark: at(mark),
        fill: at(fill),
        wash: at(wash),
    }
}

pub(super) fn diff_colors(mode: Mode) -> DiffColors {
    match mode {
        Mode::Light => light(),
        Mode::Dark => dark(),
    }
}

fn light() -> DiffColors {
    DiffColors {
        delete: tone(
            27.,
            [(0.50, 0.17), (0.62, 0.17), (0.90, 0.06), (0.945, 0.04)],
        ),
        add: tone(
            150.,
            [(0.46, 0.12), (0.60, 0.14), (0.895, 0.07), (0.945, 0.045)],
        ),
        change: DiffTone {
            wash: oklch(0.95, 0.05, 80.),
            ..tone(
                70.,
                [(0.48, 0.11), (0.60, 0.13), (0.90, 0.065), (0.95, 0.05)],
            )
        },
        moved: tone(
            210.,
            [(0.48, 0.10), (0.60, 0.10), (0.90, 0.05), (0.945, 0.035)],
        ),
        conflict: tone(
            335.,
            [(0.48, 0.16), (0.56, 0.18), (0.90, 0.06), (0.945, 0.04)],
        ),
        auto_merged: tone(
            262.,
            [(0.50, 0.15), (0.50, 0.15), (0.90, 0.05), (0.945, 0.035)],
        ),
        gap_hatch: oklch(0.91, 0.006, 260.),
    }
}

fn dark() -> DiffColors {
    DiffColors {
        delete: tone(
            27.,
            [(0.76, 0.13), (0.66, 0.16), (0.36, 0.09), (0.27, 0.05)],
        ),
        add: tone(
            150.,
            [(0.78, 0.13), (0.68, 0.14), (0.36, 0.08), (0.27, 0.05)],
        ),
        change: tone(
            78.,
            [(0.82, 0.12), (0.74, 0.13), (0.36, 0.08), (0.28, 0.045)],
        ),
        moved: tone(
            210.,
            [(0.78, 0.09), (0.68, 0.10), (0.36, 0.06), (0.27, 0.04)],
        ),
        conflict: tone(
            335.,
            [(0.78, 0.13), (0.70, 0.16), (0.36, 0.09), (0.27, 0.05)],
        ),
        auto_merged: tone(
            262.,
            [(0.72, 0.13), (0.72, 0.13), (0.35, 0.08), (0.27, 0.045)],
        ),
        gap_hatch: oklch(0.23, 0.01, 260.),
    }
}
