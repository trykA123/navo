use std::{
    ffi::OsStr,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn library() -> PathBuf {
    let executable = std::env::current_exe().unwrap();
    let dependencies = executable.parent().unwrap();
    std::fs::read_dir(dependencies)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("libnavo_bench-")
                && path.extension() == Some(OsStr::new("rlib"))
        })
        .unwrap()
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
        .arg("-L")
        .arg(format!(
            "dependency={}",
            library.parent().unwrap().display()
        ))
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
