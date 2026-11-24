//! Nested Settings rows: which settings sit indented under a parent setting, and whether
//! that parent is on, so the row view can indent and dim them (README "every setting" map).

use super::fields::Field;
use super::values::Values;
use crate::modifier_hints::HintsMode;
use crate::prelude::*;
use crate::tokens::Theme;

/// Left indent of a nested row in px (README `SET_INDENT`).
pub(super) const INDENT: f32 = 22.;
/// The guide line sits this far left of the nested row's edge, inside the indent (px).
const GUIDE_LEFT: f32 = -12.;

/// The TOML name of the "Custom" choice a command row or the inspector toggle needs.
const CUSTOM: &str = "custom";

impl Field {
    /// True for a row that sits indented under the setting it depends on.
    pub fn is_nested(self) -> bool {
        matches!(
            self,
            Self::SortDescending
                | Self::RetentionDays
                | Self::RetentionCapMib
                | Self::RetentionMinFreeMib
                | Self::AutomotiveInspector
                | Self::TerminalCommand
                | Self::GitToolCommand
                | Self::GitFetchMinutes
                | Self::ClipboardRecord
                | Self::ClipboardKey
                | Self::ClipboardPosition
                | Self::ModifierHintsDelay
        )
    }
}

impl Values {
    /// False for a nested row while the setting it needs is off; the row is then dimmed and
    /// its value cannot change.
    pub fn is_enabled(&self, field: Field) -> bool {
        match field {
            Field::RetentionDays | Field::RetentionCapMib | Field::RetentionMinFreeMib => {
                self.flag(Field::RetainDeletes) || self.flag(Field::RetainOverwrites)
            }
            Field::AutomotiveInspector => self.toml_of(Field::AutomotivePreset) == CUSTOM,
            Field::TerminalCommand => self.terminal_provider() == CUSTOM,
            Field::GitFetchMinutes => self.git_fetch(),
            Field::ClipboardRecord | Field::ClipboardKey | Field::ClipboardPosition => {
                self.clipboard_on()
            }
            Field::ModifierHintsDelay => self.modifier_hints() != HintsMode::Off,
            _ => true,
        }
    }
}

/// Indents `row` under its parent and draws the thin guide line beside it, when `is_nested`.
pub(super) fn indented<E: Styled + ParentElement>(t: &Theme, row: E, is_nested: bool) -> E {
    if !is_nested {
        return row;
    }
    let guide = div()
        .absolute()
        .left(px(GUIDE_LEFT))
        .top(px(-2.))
        .bottom(px(-2.))
        .w(px(1.))
        .bg(t.colors.line);
    row.relative().ml(px(INDENT)).child(guide)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Change, Value};

    fn with(values: Values, field: Field, value: Value) -> Values {
        values.with(&Change { field, value })
    }

    #[test]
    fn descending_and_every_dependent_row_sit_under_their_parent() {
        assert!(Field::SortDescending.is_nested());
        assert!(Field::RetentionDays.is_nested());
        assert!(Field::TerminalCommand.is_nested());
        assert!(Field::ClipboardPosition.is_nested());
        assert!(Field::ModifierHintsDelay.is_nested());
        assert!(!Field::Sort.is_nested());
        assert!(!Field::GitNetwork.is_nested());
    }

    #[test]
    fn a_nested_row_is_enabled_only_while_its_parent_is_on() {
        let off = with(Values::default(), Field::ClipboardOn, Value::Flag(false));
        assert!(!off.is_enabled(Field::ClipboardRecord));
        let on = with(off, Field::ClipboardOn, Value::Flag(true));
        assert!(on.is_enabled(Field::ClipboardRecord));
        let hints_off = with(Values::default(), Field::ModifierHints, Value::Pick(0));
        assert!(!hints_off.is_enabled(Field::ModifierHintsDelay));
        assert!(Values::default().is_enabled(Field::ModifierHintsDelay));
    }

    #[test]
    fn the_retention_limits_need_either_kind_of_kept_file() {
        let none = with(Values::default(), Field::RetainDeletes, Value::Flag(false));
        let none = with(none, Field::RetainOverwrites, Value::Flag(false));
        assert!(!none.is_enabled(Field::RetentionDays));
        let overwrites = with(none, Field::RetainOverwrites, Value::Flag(true));
        assert!(overwrites.is_enabled(Field::RetentionMinFreeMib));
    }

    #[test]
    fn a_command_row_needs_its_custom_choice() {
        assert!(!Values::default().is_enabled(Field::TerminalCommand));
        let custom = with(Values::default(), Field::TerminalProvider, Value::Pick(4));
        assert!(custom.is_enabled(Field::TerminalCommand));
        assert!(!Values::default().is_enabled(Field::AutomotiveInspector));
        let custom = with(Values::default(), Field::AutomotivePreset, Value::Pick(2));
        assert!(custom.is_enabled(Field::AutomotiveInspector));
        assert!(Values::default().is_enabled(Field::SortDescending));
    }
}
