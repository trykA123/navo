use gpui_kit::Action;

use super::{
    ShowLineEnds, ToggleCharts, ToggleInfo, ToggleLineNumbers, ToggleMemoryStrip,
    ToggleOpensInspector, ToggleRowBars, ViewCustom, ViewRich, ViewSimple,
};
use crate::settings::{Change, Field, PRESETS, Value, Values};

pub struct Part {
    pub field: Field,
    pub action: fn() -> Box<dyn Action>,
}

pub struct PartSet {
    pub preset: Option<Field>,
    pub parts: &'static [Part],
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

pub(in crate::window_view) const TEXT: PartSet = PartSet {
    preset: None,
    parts: &[
        Part {
            field: Field::TextLineNumbers,
            action: || Box::new(ToggleLineNumbers),
        },
        Part {
            field: Field::TextLineEnds,
            action: || Box::new(ShowLineEnds),
        },
    ],
    right_drawer: None,
};

pub(in crate::window_view) const IMAGE: PartSet = PartSet {
    preset: None,
    parts: &[Part {
        field: Field::ImageDetails,
        action: || Box::new(ToggleInfo),
    }],
    right_drawer: Some(Field::ImageDetails),
};

pub(in crate::window_view) const AUTOMOTIVE: PartSet = PartSet {
    preset: Some(Field::AutomotivePreset),
    parts: &[
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
    ],
    right_drawer: None,
};

fn custom() -> usize {
    PRESETS
        .iter()
        .position(|choice| choice.toml == "custom")
        .unwrap_or(PRESETS.len() - 1)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::window_view) struct ViewParts {
    overrides: Vec<Change>,
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

    pub fn toggle(&mut self, set: &PartSet, field: Field, settings: &Values) {
        let shown = self.effective(settings);
        let is_on = shown.is_shown(set.preset, field);
        if let Some(preset) = set.preset {
            for part in set.parts {
                self.set(Change {
                    field: part.field,
                    value: Value::Flag(shown.is_shown(Some(preset), part.field)),
                });
            }
            self.set(Change {
                field: preset,
                value: Value::Pick(custom()),
            });
        }
        self.set(Change {
            field,
            value: Value::Flag(!is_on),
        });
    }

    pub fn choose(&mut self, set: &PartSet, at: usize) {
        if let Some(preset) = set.preset {
            self.set(Change {
                field: preset,
                value: Value::Pick(at.min(PRESETS.len() - 1)),
            });
        }
    }

    pub fn keep(&mut self, settings: &Values) -> Vec<Change> {
        std::mem::take(&mut self.overrides)
            .into_iter()
            .filter(|change| settings.get(change.field) != &change.value)
            .collect()
    }

    pub fn reset(&mut self) {
        self.overrides.clear();
    }
}

#[cfg(test)]
mod tests;
