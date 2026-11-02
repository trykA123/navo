use super::fields::{Control, F, FieldSpec, S, spec};

pub(super) const TRAY_FIELDS: [FieldSpec; 2] = [
    spec(
        (F::KeepInTray, S::Tray, "tray.keep-running"),
        ("settings-keep-in-tray", "settings-keep-in-tray-help"),
        Control::Toggle,
    ),
    spec(
        (F::SummonKey, S::Tray, "tray.summon-key"),
        ("settings-summon-key", "settings-summon-key-help"),
        Control::Text,
    ),
];
