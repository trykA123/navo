use gpui_kit::Hsla;

use crate::browser::Tone;
use crate::tokens::Theme;

/// The text colour of a tone, from the diff tokens.
pub(super) fn text(t: &Theme, tone: Tone) -> Hsla {
    match tone {
        Tone::Muted => t.colors.muted,
        Tone::Change => t.diff.change.text,
        Tone::Delete => t.diff.delete.text,
        Tone::Add => t.diff.add.text,
        Tone::Conflict => t.diff.conflict.text,
        Tone::Busy => t.colors.accent_text,
    }
}

/// The badge background of a tone; quiet tones have none.
pub(super) fn wash(t: &Theme, tone: Tone) -> Option<Hsla> {
    match tone {
        Tone::Muted | Tone::Busy => None,
        Tone::Change => Some(t.diff.change.wash),
        Tone::Delete => Some(t.diff.delete.wash),
        Tone::Add => Some(t.diff.add.wash),
        Tone::Conflict => Some(t.diff.conflict.wash),
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::Hsla;

    use super::{text, wash};
    use crate::browser::Tone;
    use crate::tokens::{Mode, Theme, contrast_ratio};

    const TONES: [Tone; 6] = [
        Tone::Muted,
        Tone::Change,
        Tone::Delete,
        Tone::Add,
        Tone::Conflict,
        Tone::Busy,
    ];

    fn grounds(t: &Theme, tone: Tone) -> Vec<(&'static str, Hsla)> {
        let mut grounds = vec![
            ("card", t.surfaces.card),
            ("current", t.surfaces.current),
            ("gap", t.diff.gap_hatch),
        ];
        grounds.extend(wash(t, tone).map(|badge| ("badge", badge)));
        grounds
    }

    #[test]
    fn every_tone_reads_on_the_pane_the_selection_and_its_own_badge() {
        let themes = [Mode::Light, Mode::Dark].map(|mode| (mode, Theme::new(mode)));
        let cases: Vec<_> = themes
            .iter()
            .flat_map(|theme| TONES.map(|tone| (theme, tone)))
            .collect();
        for ((mode, t), tone) in cases {
            for (name, ground) in grounds(t, tone) {
                let ratio = contrast_ratio(text(t, tone), ground);
                assert!(ratio >= 4.5, "{mode:?} {tone:?} on {name} is {ratio:.2}:1");
            }
        }
    }
}
