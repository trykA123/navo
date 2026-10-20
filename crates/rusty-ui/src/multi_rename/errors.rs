use rusty_core::rename::{Mask, RenameRules, RuleError, RuleField};
use rusty_l10n::{t, tr};

use super::view::fields::FieldErrors;

#[must_use]
pub fn field_errors(rules: &RenameRules, error: &RuleError) -> FieldErrors {
    let mut errors = FieldErrors::default();
    match error.field {
        RuleField::NameMask => errors.name_mask = Some(mask_error(&rules.name_mask).into()),
        RuleField::ExtMask => errors.ext_mask = Some(mask_error(&rules.ext_mask).into()),
        RuleField::Find => errors.find = Some(tr("rename-error-regex").into()),
        RuleField::Counter => errors.counter = Some(tr("rename-error-counter").into()),
    }
    errors
}

#[must_use]
pub fn counter_error() -> FieldErrors {
    FieldErrors {
        counter: Some(tr("rename-error-counter").into()),
        ..FieldErrors::default()
    }
}

fn mask_error(text: &str) -> String {
    let at = Mask::parse(text).err().map_or(1, |error| error.at);
    let token: String = text
        .chars()
        .skip(at.saturating_sub(1))
        .scan(false, |closed, ch| {
            (!*closed).then(|| {
                *closed = ch == ']';
                ch
            })
        })
        .collect();
    t!("rename-error-token", token = token, position = at)
}
