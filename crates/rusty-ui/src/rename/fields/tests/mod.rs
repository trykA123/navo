mod exif;
mod file;

use rusty_core::rename::FieldRegistry;

use super::builtin;

const FORBIDDEN: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

fn registry() -> FieldRegistry {
    builtin()
}

#[test]
fn the_builtin_registry_lists_file_and_exif_with_their_fields() {
    let registry = registry();
    assert_eq!(registry.groups().collect::<Vec<_>>(), ["file", "exif"]);
    let listed: Vec<String> = registry
        .listing()
        .map(|entry| format!("{}.{}", entry.group, entry.field.name))
        .collect();
    assert_eq!(
        listed,
        [
            "file.size",
            "file.size_kb",
            "file.created",
            "file.accessed",
            "file.attrs",
            "exif.date",
            "exif.camera",
            "exif.model",
            "exif.width",
            "exif.height",
        ]
    );
}

#[test]
fn every_field_has_a_label_id() {
    for entry in registry().listing() {
        assert!(entry.field.label_id.starts_with("rename-field-"));
    }
}

fn assert_safe(value: &str) {
    assert!(!value.is_empty());
    assert!(!value.contains(FORBIDDEN), "{value:?}");
    assert!(!value.chars().any(char::is_control), "{value:?}");
}

pub(super) fn assert_all_safe(values: &rusty_core::rename::Values, fields: &[&str]) {
    for field in fields {
        if let Some(value) = values.get(field) {
            assert_safe(value);
        }
    }
}
