use super::*;

const ONE_ERROR_PER_KIND: &[&str] = &[
    "pane.back {",
    "pane.back x",
    "pane.fly",
    "selection.range { row: 1 }",
    "selection.range { to: 1, to: 2 }",
    "pane.navigate {}",
    "pane.navigate { location: 5 }",
    "selection.range { to: -1 }",
    r#"selection.add-pattern { pattern: "\q" }"#,
    r#"pane.navigate { location: "C:\\Users"#,
];

fn with_shared_snapshot_dir(check: impl FnOnce()) {
    insta::with_settings!({ snapshot_path => "../snapshots", prepend_module_to_snapshot => false }, {
        check();
    });
}

#[test]
fn every_command_prints_in_canonical_form() {
    let lines: Vec<String> = CANONICAL_EXAMPLES
        .iter()
        .map(|t| match parse(t) {
            Ok(c) => c.to_string(),
            Err(e) => format!("error: {e}"),
        })
        .collect();
    with_shared_snapshot_dir(|| insta::assert_snapshot!("commands_printed", lines.join("\n")));
}

#[test]
fn every_error_kind_has_a_readable_message() {
    let lines: Vec<String> = ONE_ERROR_PER_KIND
        .iter()
        .map(|t| match parse(t) {
            Ok(c) => format!("{t:?} => parsed as {c:?}"),
            Err(e) => format!("{t:?} => {e}"),
        })
        .collect();
    with_shared_snapshot_dir(|| insta::assert_snapshot!("errors_display", lines.join("\n")));
}
