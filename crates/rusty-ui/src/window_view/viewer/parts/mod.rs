use gpui_kit::Action;

use super::{
    ShowLineEnds, ToggleCharts, ToggleInfo, ToggleLineNumbers, ToggleMemoryStrip,
    ToggleOpensInspector, TogglePreview, ToggleRowBars, ViewCustom, ViewRich, ViewSimple,
};
use crate::settings::{Change, Field, PRESETS, Value, Values};

pub struct Part {
    pub field: Field,
    pub action: fn() -> Box<dyn Action>,
}

pub struct PartSet {
    pub preset: Option<Field>,
    pub parts: &'static [Part],
    pub family: &'static [Part],
    pub right_drawer: Option<Field>,
}

impl PartSet {
    pub fn has(&self, field: Field) -> bool {
        self.parts.iter().any(|part| part.field == field)
    }
}

pub(in crate::window_view) const PRESET_ACTIONS: [fn() -> Box<dyn Action>; 3] = [
    || Box::new(ViewSimple),
    || Box::new(ViewRich),
    || Box::new(ViewCustom),
];

const TEXT_PARTS: &[Part] = &[
    Part {
        field: Field::TextLineNumbers,
        action: || Box::new(ToggleLineNumbers),
    },
    Part {
        field: Field::TextLineEnds,
        action: || Box::new(ShowLineEnds),
    },
];

pub(in crate::window_view) const TEXT: PartSet = PartSet {
    preset: None,
    parts: TEXT_PARTS,
    family: TEXT_PARTS,
    right_drawer: None,
};

const MARKDOWN_TEXT_PARTS: &[Part] = &[
    Part {
        field: Field::TextLineNumbers,
        action: || Box::new(ToggleLineNumbers),
    },
    Part {
        field: Field::TextLineEnds,
        action: || Box::new(ShowLineEnds),
    },
    Part {
        field: Field::MarkdownPreviewSplit,
        action: || Box::new(TogglePreview),
    },
];

pub(in crate::window_view) const MARKDOWN_TEXT: PartSet = PartSet {
    preset: None,
    parts: MARKDOWN_TEXT_PARTS,
    family: MARKDOWN_TEXT_PARTS,
    right_drawer: None,
};

const IMAGE_PARTS: &[Part] = &[Part {
    field: Field::ImageDetails,
    action: || Box::new(ToggleInfo),
}];

pub(in crate::window_view) const IMAGE: PartSet = PartSet {
    preset: None,
    parts: IMAGE_PARTS,
    family: IMAGE_PARTS,
    right_drawer: Some(Field::ImageDetails),
};

const AUTOMOTIVE_PARTS: &[Part] = &[
    Part {
        field: Field::AutomotiveInspector,
        action: || Box::new(ToggleOpensInspector),
    },
    Part {
        field: Field::AutomotiveMemoryStrip,
        action: || Box::new(ToggleMemoryStrip),
    },
    Part {
        field: Field::AutomotiveCharts,
        action: || Box::new(ToggleCharts),
    },
    Part {
        field: Field::AutomotiveRowBars,
        action: || Box::new(ToggleRowBars),
    },
];

pub(in crate::window_view) const AUTOMOTIVE: PartSet = PartSet {
    preset: Some(Field::AutomotivePreset),
    parts: AUTOMOTIVE_PARTS,
    family: AUTOMOTIVE_PARTS,
    right_drawer: None,
};

pub(in crate::window_view) const AUTOMOTIVE_TABLE: PartSet = PartSet {
    preset: Some(Field::AutomotivePreset),
    parts: &[Part {
        field: Field::AutomotiveInspector,
        action: || Box::new(ToggleOpensInspector),
    }],
    family: AUTOMOTIVE_PARTS,
    right_drawer: None,
};

fn preset_at(toml: &str) -> usize {
    PRESETS
        .iter()
        .position(|choice| choice.toml == toml)
        .unwrap_or(PRESETS.len() - 1)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::window_view) struct ViewParts {
    overrides: Vec<Change>,
    keeping: Vec<Change>,
    keep_failed: bool,
}

impl ViewParts {
    pub fn effective(&self, settings: &Values) -> Values {
        self.overrides
            .iter()
            .fold(settings.clone(), |values, change| values.with(change))
    }

    pub fn is_shown(&self, set: &PartSet, field: Field, settings: &Values) -> bool {
        self.effective(settings).is_shown(set.preset, field)
    }

    fn set(&mut self, change: Change) {
        self.overrides.retain(|older| older.field != change.field);
        self.overrides.push(change);
    }

    fn set_parts(&mut self, parts: &[Part], shown: impl Fn(Field) -> bool) {
        for part in parts {
            self.set(Change {
                field: part.field,
                value: Value::Flag(shown(part.field)),
            });
        }
    }

    fn pick_preset(&mut self, set: &PartSet, at: usize) {
        if let Some(preset) = set.preset {
            self.set(Change {
                field: preset,
                value: Value::Pick(at),
            });
        }
    }

    fn is_custom(&self, set: &PartSet, settings: &Values) -> bool {
        let custom = Value::Pick(preset_at("custom"));
        set.preset
            .is_some_and(|preset| self.effective(settings).get(preset) == &custom)
    }

    fn make_custom(&mut self, set: &PartSet, settings: &Values) {
        if set.preset.is_none() || self.is_custom(set, settings) {
            return;
        }
        let shown = self.effective(settings);
        self.set_parts(set.family, |field| shown.is_shown(set.preset, field));
        self.pick_preset(set, preset_at("custom"));
    }

    pub fn toggle(&mut self, set: &PartSet, field: Field, settings: &Values) {
        let is_on = self.is_shown(set, field, settings);
        self.make_custom(set, settings);
        self.set(Change {
            field,
            value: Value::Flag(!is_on),
        });
    }

    pub fn choose(&mut self, set: &PartSet, at: usize, settings: &Values) {
        let at = at.min(PRESETS.len() - 1);
        if at == preset_at("custom") {
            self.make_custom(set, settings);
        } else if set.preset.is_some() {
            self.set_parts(set.parts, |_| at == preset_at("rich"));
        }
        self.pick_preset(set, at);
    }

    pub fn keep(&mut self, settings: &Values) -> Vec<Change> {
        let kept: Vec<Change> = self
            .overrides
            .iter()
            .filter(|change| settings.get(change.field) != &change.value)
            .cloned()
            .collect();
        if self.keeping.is_empty() {
            self.keep_failed = false;
        }
        self.keeping
            .retain(|pending| kept.iter().all(|change| change.field != pending.field));
        self.keeping.extend(kept.iter().cloned());
        kept
    }

    pub fn is_pending(&self) -> bool {
        !self.keeping.is_empty()
    }

    pub fn settle(&mut self, sent: &[Change], is_clean: bool) -> Option<bool> {
        let done: Vec<Change> = self
            .keeping
            .iter()
            .filter(|change| sent.contains(change))
            .cloned()
            .collect();
        if done.is_empty() {
            return None;
        }
        self.keeping.retain(|change| !done.contains(change));
        if is_clean {
            self.overrides.retain(|change| !done.contains(change));
        } else {
            self.keep_failed = true;
        }
        self.keeping.is_empty().then_some(!self.keep_failed)
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests;
