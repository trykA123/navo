use super::fields::{Choice, Control, F, FieldSpec, Look, S, choice, spec};

const POSITIONS: [Choice; 3] = [
    choice("settings-clipboard-position-last", "last"),
    choice("settings-clipboard-position-cursor", "cursor"),
    choice("settings-clipboard-position-center", "center"),
];

pub(super) const CLIPBOARD_FIELDS: [FieldSpec; 4] = [
    spec(
        (F::ClipboardOn, S::Clipboard, "clipboard.enabled"),
        ("settings-clipboard-on", "settings-clipboard-on-help"),
        Control::Toggle,
    ),
    spec(
        (F::ClipboardRecord, S::Clipboard, "clipboard.record-history"),
        (
            "settings-clipboard-record",
            "settings-clipboard-record-help",
        ),
        Control::Toggle,
    ),
    spec(
        (F::ClipboardKey, S::Clipboard, "clipboard.summon-key"),
        ("settings-clipboard-key", "settings-clipboard-key-help"),
        Control::Text,
    ),
    spec(
        (
            F::ClipboardPosition,
            S::Clipboard,
            "clipboard.popup-position",
        ),
        (
            "settings-clipboard-position",
            "settings-clipboard-position-help",
        ),
        Control::Choices(&POSITIONS, Look::Segments),
    ),
];
