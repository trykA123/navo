//! Unit tests for the dependency-graph guard (WS1), fed synthetic metadata.

use super::deps::violations;

/// Build `cargo metadata` JSON from (crate, [dependencies]) pairs.
fn metadata(packages: &[(&str, &[&str])]) -> String {
    let packages: Vec<serde_json::Value> = packages
        .iter()
        .map(|(name, deps)| {
            let deps: Vec<serde_json::Value> = deps
                .iter()
                .map(|dep| serde_json::json!({ "name": dep, "kind": null }))
                .collect();
            serde_json::json!({ "name": name, "dependencies": deps })
        })
        .collect();
    serde_json::json!({ "packages": packages }).to_string()
}

#[test]
fn seams_deps_pass_clean_graph() {
    let json = metadata(&[
        ("sidewise-ui", &["sidewise-core"]),
        ("navo-app", &["navo-ui"]),
    ]);
    assert_eq!(violations(&json, &[]), Ok(Vec::new()));
}

#[test]
fn seams_deps_flag_sidewise_on_navo_app_and_ui() {
    let json = metadata(&[("sidewise-ui", &["navo-ui", "navo-app", "navo-ui"])]);
    assert_eq!(
        violations(&json, &[]),
        Ok(vec![
            "sidewise-ui -> navo-app: no sidewise crate may depend on navo-app or navo-ui"
                .to_owned(),
            "sidewise-ui -> navo-ui: no sidewise crate may depend on navo-app or navo-ui"
                .to_owned(),
        ])
    );
}

#[test]
fn seams_deps_flag_automotive_edge_unless_allowlisted() {
    let json = metadata(&[("navo-ui", &["navo-automotive"])]);
    assert_eq!(
        violations(&json, &[]),
        Ok(vec![
            "navo-ui -> navo-automotive: edge not in the allowlist".to_owned()
        ])
    );
    assert_eq!(
        violations(&json, &[("navo-ui", "navo-automotive")]),
        Ok(Vec::new())
    );
}

#[test]
fn seams_deps_reject_bad_json() {
    assert!(violations("{}", &[]).is_err());
}
