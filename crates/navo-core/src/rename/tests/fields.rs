use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use crate::rename::{
    Compiled, FieldInfo, FieldProvider, FieldRegistry, RenameRules, RuleField, Subject, Values,
    groups_needed, split_name,
};

struct Fixed {
    group: &'static str,
    fields: Vec<FieldInfo>,
    value: &'static str,
}

impl Fixed {
    fn arc(group: &'static str, value: &'static str) -> Arc<dyn FieldProvider> {
        Arc::new(Self {
            group,
            fields: vec![
                FieldInfo::builtin("one", "field-test-one"),
                FieldInfo::builtin("two", "field-test-two"),
            ],
            value,
        })
    }
}

impl FieldProvider for Fixed {
    fn group(&self) -> &str {
        self.group
    }

    fn fields(&self) -> &[FieldInfo] {
        &self.fields
    }

    fn read(&self, _path: &Path) -> Values {
        let mut values = Values::default();
        values.insert("one", self.value);
        values
    }
}

fn rules(name_mask: &str, ext_mask: &str) -> RenameRules {
    RenameRules {
        name_mask: name_mask.to_owned(),
        ext_mask: ext_mask.to_owned(),
        ..RenameRules::default()
    }
}

fn registry() -> FieldRegistry {
    let mut registry = FieldRegistry::new();
    registry.register(Fixed::arc("alpha", "a1"));
    registry.register(Fixed::arc("beta", "b1"));
    registry
}

#[test]
fn registry_lists_groups_and_fields_in_order() {
    let registry = registry();
    assert_eq!(registry.groups().collect::<Vec<_>>(), ["alpha", "beta"]);
    let listed: Vec<String> = registry
        .listing()
        .map(|entry| {
            format!(
                "{}.{}={}",
                entry.group, entry.field.name, entry.field.label_id
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            "alpha.one=field-test-one",
            "alpha.two=field-test-two",
            "beta.one=field-test-one",
            "beta.two=field-test-two",
        ]
    );
}

#[test]
fn registering_a_group_again_replaces_it() {
    let mut registry = registry();
    registry.register(Fixed::arc("alpha", "new"));
    assert_eq!(registry.groups().count(), 2);
    let needed = BTreeSet::from(["alpha".to_owned()]);
    let fields = registry.read(&needed, Path::new("x"));
    assert_eq!(fields.get("alpha", "one").as_deref(), Some("new"));
}

#[test]
fn rules_without_field_tokens_need_no_group() {
    let needed = groups_needed(&rules("[N]_[C]-[Y]", "[E]")).unwrap();
    assert!(needed.is_empty());
    assert!(groups_needed(&RenameRules::default()).unwrap().is_empty());
}

#[test]
fn rules_list_each_group_once_across_both_masks() {
    let needed =
        groups_needed(&rules("[=beta.one]-[=alpha.one]-[=beta.two]", "[=gamma.x]")).unwrap();
    assert_eq!(
        needed.into_iter().collect::<Vec<_>>(),
        ["alpha", "beta", "gamma"]
    );
}

#[test]
fn a_bad_mask_reports_its_field() {
    let error = groups_needed(&rules("[N]", "[=oops]")).unwrap_err();
    assert_eq!(error.field, RuleField::ExtMask);
}

#[test]
fn only_needed_groups_are_read() {
    let registry = registry();
    let needed = BTreeSet::from(["beta".to_owned(), "missing".to_owned()]);
    let fields = registry.read(&needed, Path::new("x"));
    assert_eq!(fields.get("beta", "one").as_deref(), Some("b1"));
    assert_eq!(fields.get("alpha", "one"), None);
    assert_eq!(fields.get("missing", "one"), None);
    assert_eq!(fields.get("beta", "two"), None);
}

#[test]
fn read_values_feed_the_mask() {
    let registry = registry();
    let rules = rules("[N]-[=alpha.one]-[=alpha.two]", "[E]");
    let needed = groups_needed(&rules).unwrap();
    let item = registry.read(&needed, Path::new("x"));
    let lookup = |group: &str, field: &str| item.get(group, field);
    let (stem, ext) = split_name("pic.jpg");
    let subject = Subject {
        stem,
        ext,
        parent: "",
        grand: "",
        time: None,
        fields: Some(&lookup),
    };
    let compiled = Compiled::new(&rules).unwrap();
    assert_eq!(compiled.new_name(&subject, 0), "pic-a1-.jpg");
}

#[test]
fn values_replace_characters_a_file_name_cannot_hold() {
    let mut values = Values::default();
    values.insert("one", "a\\b/c:d*e?f\"g<h>i|j\nk");
    values.insert("blank", "  ");
    assert_eq!(values.get("one"), Some("a_b_c_d_e_f_g_h_i_j_k"));
    assert_eq!(values.get("blank"), None);
}

#[test]
fn providers_are_shareable_between_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<FieldRegistry>();
    assert_send_sync::<Arc<dyn FieldProvider>>();
}
