use super::fields::{Control, F, FieldSpec, S, spec};

/// The Help page draws this row itself; the generic lists skip it.
pub(super) const UPDATE_FIELDS: [FieldSpec; 1] = [spec(
    (F::CheckUpdates, S::Help, "updates.check"),
    ("settings-check-updates", "settings-check-updates-help"),
    Control::Toggle,
)];
