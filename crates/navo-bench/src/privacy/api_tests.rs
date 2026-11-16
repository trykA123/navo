use std::{
    ffi::OsStr,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// Finds the `libnavo_bench-*.rlib` the compile-fail probes link against.
/// Older cargo puts it beside the test executable (`deps/`); newer cargo gives each unit its own
/// folder, so the fallback searches `<profile>/build/navo-bench/*/out/` and takes the newest.
/// Panics when neither place has it.
fn library() -> PathBuf {
    let executable = std::env::current_exe().unwrap();
    let dependencies = executable.parent().unwrap();
    if let Some(found) = find_rlib(dependencies) {
        return found;
    }
    let build = dependencies
        .ancestors()
        .find(|dir| dir.join("build").is_dir())
        .map(|dir| dir.join("build").join("navo-bench"));
    let newest = build.as_deref().and_then(|root| {
        std::fs::read_dir(root)
            .ok()?
            .filter_map(|unit| find_rlib(&unit.ok()?.path().join("out")))
            .max_by_key(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok())
    });
    newest.unwrap_or_else(|| {
        panic!(
            "libnavo_bench-*.rlib not found in {} or under <profile>/build/navo-bench/*/out/",
            dependencies.display()
        )
    })
}

/// Returns the first `libnavo_bench-*.rlib` directly inside `dir`, or `None`.
fn find_rlib(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("libnavo_bench-")
                && path.extension() == Some(OsStr::new("rlib"))
        })
}

/// Lists the folders rustc searches for the library's dependencies: its own folder, plus every
/// `build/*/*/out` folder when the library sits in the per-unit layout.
fn search_dirs(library: &Path) -> Vec<PathBuf> {
    let own = library.parent().unwrap();
    let mut dirs = vec![own.to_path_buf()];
    let build = own.ancestors().nth(3).filter(|dir| dir.ends_with("build"));
    for package in build
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .into_iter()
        .flatten()
    {
        let units = std::fs::read_dir(package.unwrap().path())
            .into_iter()
            .flatten();
        dirs.extend(units.map(|unit| unit.unwrap().path().join("out")));
    }
    dirs
}

#[expect(
    clippy::disallowed_methods,
    reason = "the compile-fail probe runs rustc on a temporary file"
)]
fn compile(source: &str, library: &Path) -> Output {
    let mut input = tempfile::Builder::new().suffix(".rs").tempfile().unwrap();
    input.write_all(source.as_bytes()).unwrap();
    let output = tempfile::tempdir().unwrap();
    Command::new("rustc")
        .args([
            "--edition=2024",
            "--crate-name=privacy_api",
            "--crate-type=lib",
            "--emit=metadata",
        ])
        .arg(input.path())
        .arg("--extern")
        .arg(format!("navo_bench={}", library.display()))
        .args(
            search_dirs(library)
                .into_iter()
                .flat_map(|dir| ["-L".to_owned(), format!("dependency={}", dir.display())]),
        )
        .arg("--out-dir")
        .arg(output.path())
        .output()
        .unwrap()
}

#[test]
fn public_api_accepts_fixed_labels_but_rejects_runtime_text_and_raw_bytes() {
    let library = library();
    let valid = compile(
        r#"
        use navo_bench::privacy::{Label, SafeText};
        pub fn label() -> SafeText { navo_bench::safe_text!("files") }
        pub fn allowed() -> SafeText { Label::Files.into() }
    "#,
        &library,
    );
    assert!(
        valid.status.success(),
        "the compile-fail control must compile"
    );
    for (source, error_code) in rejected_programs() {
        let result = compile(source, &library);
        assert!(
            !result.status.success(),
            "a runtime output sink must not compile"
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains(error_code));
    }
}

fn rejected_programs() -> [(&'static str, &'static str); 5] {
    [
        (
            r#"use navo_bench::privacy::SafeText;
            pub fn label(value: String) -> SafeText { navo_bench::safe_text!(value.as_str()) }"#,
            "E0435",
        ),
        (
            r#"use navo_bench::privacy::SafeText;
            pub fn label(value: &str) -> SafeText { navo_bench::safe_text!(value) }"#,
            "E0435",
        ),
        (
            r#"use navo_bench::privacy::SafeText;
            pub fn label() -> SafeText { SafeText("path") }"#,
            "E0423",
        ),
        (
            r#"use navo_bench::privacy::{SafeWriter, Output};
            pub fn write(writer: &mut SafeWriter<Vec<u8>>, value: String) {
                let _ = writer.write(Output::Json, value.as_bytes());
            }"#,
            "E0308",
        ),
        (
            r#"use navo_bench::privacy::SafeText;
            pub fn label(value: String) -> SafeText { navo_bench::safe_text!(Box::leak(value.into_boxed_str())) }"#,
            "E0435",
        ),
    ]
}
