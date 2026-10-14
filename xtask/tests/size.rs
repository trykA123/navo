#[cfg(test)]
mod tests {
    use std::process::Command;

    #[test]
    fn size_checks_the_repository_and_prints_the_file_count() {
        let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("size")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).unwrap();
        let count = stdout
            .strip_prefix("size: ")
            .unwrap()
            .strip_suffix(" files checked\n")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert!(count > 0);
    }
}
