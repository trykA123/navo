//! State, Lucide glyph filtering, and input normalization for the button bar icon maker.

use std::rc::Rc;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{App, Context, Entity, Subscription, Window};
use navo_icons::registry::registry;
use navo_icons::search::{SetFilter, TagIndex, search};
use navo_icons::spec::{IconSpec, Shape, Tone};
use navo_l10n::tr;

use crate::primitives::{ListScroll, TextFieldState, text_field_state};
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::BarIcon;

pub(super) const GLYPHS_PER_ROW: usize = 8;
const DEFAULT_GLYPH: &str = "folder";

/// Active icon maker sub-mode: letter badge or Lucide glyph with optional dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum MakerMode {
    #[default]
    Letters,
    Glyph,
}

/// Persistent inputs, colour/shape selections, and filtered Lucide rows for the icon maker.
pub(crate) struct MakerState {
    pub(super) mode: MakerMode,
    pub(super) letters_input: Entity<TextFieldState>,
    pub(super) shape: Shape,
    pub(super) letter_tone: Tone,
    pub(super) glyph_search: Entity<TextFieldState>,
    pub(super) glyph_name: String,
    pub(super) glyph_tone: Tone,
    pub(super) glyph_dot: Option<Tone>,
    pub(super) glyph_rows: Rc<[Vec<String>]>,
    pub(super) glyph_scroll: ListScroll,
    _subs: Vec<Subscription>,
}

impl MakerState {
    /// Creates maker inputs with default shape/tones and subscribes to query/text changes.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<NavoWindow>) -> Self {
        let letters_input =
            text_field_state(tr("button-bar-maker-letters-placeholder"), window, cx);
        let glyph_search = text_field_state(tr("button-bar-maker-glyph-search"), window, cx);
        let subs = vec![
            cx.subscribe(&letters_input, |view, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    view.maker_letters_changed(cx);
                }
            }),
            cx.subscribe(&glyph_search, |view, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    view.maker_glyph_search_changed(cx);
                }
            }),
        ];
        Self {
            mode: MakerMode::default(),
            letters_input,
            shape: Shape::Soft,
            letter_tone: Tone::Accent,
            glyph_search,
            glyph_name: String::from(DEFAULT_GLYPH),
            glyph_tone: Tone::Text,
            glyph_dot: None,
            glyph_rows: Rc::from([]),
            glyph_scroll: ListScroll::new(),
            _subs: subs,
        }
    }

    /// Rebuilds the virtualised 8-column Lucide rows from `index` and the current search query.
    pub(crate) fn refresh_glyphs(&mut self, index: Option<&TagIndex>, cx: &App) {
        self.glyph_scroll.scroll_to_top(0);
        let Some(index) = index else {
            self.glyph_rows = Rc::from([]);
            return;
        };
        let query = self.glyph_search.read(cx).value();
        let hits = search(
            index,
            &query,
            &SetFilter::Set("lucide".to_owned()),
            usize::MAX,
        );
        let names: Vec<String> = hits.into_iter().map(|hit| hit.name).collect();
        self.glyph_rows = names
            .chunks(GLYPHS_PER_ROW)
            .map(<[String]>::to_vec)
            .collect();
    }

    /// Updates maker fields when a button with an existing `Letters` or `Glyph` spec is selected.
    pub(crate) fn sync_from_spec(
        &mut self,
        spec: Option<&IconSpec>,
        window: &mut Window,
        cx: &mut App,
    ) {
        match spec {
            Some(IconSpec::Letters {
                letters,
                shape,
                tone,
            }) => {
                self.mode = MakerMode::Letters;
                self.shape = *shape;
                self.letter_tone = *tone;
                let value = letters.clone();
                self.letters_input
                    .update(cx, |state, cx| state.set_value(value, window, cx));
            }
            Some(IconSpec::Glyph { glyph, tone, dot }) => {
                self.mode = MakerMode::Glyph;
                self.glyph_name = glyph.clone();
                self.glyph_tone = *tone;
                self.glyph_dot = *dot;
            }
            _ => {
                self.letters_input
                    .update(cx, |state, cx| state.set_value(String::new(), window, cx));
            }
        }
    }

    /// Builds the `IconSpec` represented by the current maker controls, if valid.
    pub(super) fn current_spec(&self, cx: &App) -> Option<IconSpec> {
        match self.mode {
            MakerMode::Letters => {
                let letters = normalize_letters(&self.letters_input.read(cx).value())?;
                Some(IconSpec::Letters {
                    letters,
                    shape: self.shape,
                    tone: self.letter_tone,
                })
            }
            MakerMode::Glyph => (!self.glyph_name.is_empty()).then(|| IconSpec::Glyph {
                glyph: self.glyph_name.clone(),
                tone: self.glyph_tone,
                dot: self.glyph_dot,
            }),
        }
    }

    /// Builds the live [`BarIcon`] rendered in the 16/28/48 px preview boxes.
    pub(super) fn preview_icon(&self, cx: &App) -> BarIcon {
        match self.mode {
            MakerMode::Letters => match normalize_letters(&self.letters_input.read(cx).value()) {
                Some(letters) => BarIcon::Letters {
                    letters,
                    shape: self.shape,
                    tone: self.letter_tone,
                },
                None => BarIcon::Missing { dashed: true },
            },
            MakerMode::Glyph => match registry().body("lucide", &self.glyph_name) {
                Some(svg) => BarIcon::Glyph {
                    svg,
                    tone: self.glyph_tone,
                    dot: self.glyph_dot,
                },
                None => BarIcon::Missing { dashed: false },
            },
        }
    }
}

/// Trims, validates 1–2 alphanumeric characters, and uppercases `raw` for `IconSpec::Letters`.
pub(super) fn normalize_letters(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let mut chars = trimmed.chars();
    let valid = match (chars.next(), chars.next(), chars.next()) {
        (Some(first), None, None) => first.is_alphanumeric(),
        (Some(first), Some(second), None) => first.is_alphanumeric() && second.is_alphanumeric(),
        _ => false,
    };
    if !valid {
        return None;
    }
    let upper: String = trimmed.chars().flat_map(char::to_uppercase).collect();
    (upper.chars().count() <= 2).then_some(upper)
}

pub(super) fn shape_label(shape: Shape) -> String {
    match shape {
        Shape::Soft => tr("button-bar-maker-shape-soft"),
        Shape::Circle => tr("button-bar-maker-shape-circle"),
        Shape::Outline => tr("button-bar-maker-shape-outline"),
        Shape::Solid => tr("button-bar-maker-shape-solid"),
        Shape::None => tr("button-bar-maker-shape-none"),
    }
}

pub(super) fn tone_label(tone: Tone) -> String {
    match tone {
        Tone::Accent => tr("button-bar-tone-accent"),
        Tone::Ok => tr("button-bar-tone-ok"),
        Tone::Warn => tr("button-bar-tone-warn"),
        Tone::Folder => tr("button-bar-tone-folder"),
        Tone::KindCode => tr("button-bar-tone-kind-code"),
        Tone::KindAuto => tr("button-bar-tone-kind-auto"),
        Tone::KindImage => tr("button-bar-tone-kind-image"),
        Tone::KindArchive => tr("button-bar-tone-kind-archive"),
        Tone::KindDoc => tr("button-bar-tone-kind-doc"),
        Tone::KindApp => tr("button-bar-tone-kind-app"),
        Tone::Muted => tr("button-bar-tone-muted"),
        Tone::Text => tr("button-bar-tone-text"),
    }
}
