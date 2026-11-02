use super::fields::{Control, F, FieldSpec, S, spec};

pub(super) const CLIPBOARD_FIELDS: [FieldSpec; 3] = [
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
];
