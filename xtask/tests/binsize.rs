#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        path::{Path, PathBuf},
        process::{Command, Output},
    };

    const RUSTY_EXE_LIMIT: u64 = 30 * 1024 * 1024;

    struct TestFile(PathBuf);

    impl TestFile {
        fn new(bytes: u64) -> Self {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("rusty-binsize-{}-{timestamp}", std::process::id()));
            File::create_new(&path).unwrap().set_len(bytes).unwrap();
            Self(path)
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            std::fs::remove_file(&self.0).unwrap();
        }
    }

    fn invoke(path: &Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("binsize")
            .arg(path)
            .output()
            .unwrap()
    }

    #[test]
    fn binsize_accepts_a_small_file_and_prints_its_size() {
        let file = TestFile::new(5);
        let output = invoke(&file.0);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{}: 5 bytes (limit {RUSTY_EXE_LIMIT})\n", file.0.display())
        );
    }

    #[test]
    fn binsize_rejects_a_sparse_file_one_byte_over_the_limit() {
        let bytes = RUSTY_EXE_LIMIT + 1;
        let file = TestFile::new(bytes);
        let output = invoke(&file.0);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!(
                "{}: {bytes} bytes (limit {RUSTY_EXE_LIMIT})\n",
                file.0.display()
            )
        );
    }

    #[test]
    fn binsize_reports_a_missing_path() {
        let file = TestFile::new(0);
        let missing = file.0.with_extension("missing");
        let output = invoke(&missing);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .starts_with(&format!("{}:", missing.display()))
        );
    }
}
