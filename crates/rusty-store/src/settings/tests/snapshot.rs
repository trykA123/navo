use super::*;

#[test]
fn default_settings_as_toml() {
    insta::with_settings!({ snapshot_path => "../snapshots", prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!("default_settings", to_toml(&Settings::default()));
    });
}
