#[cfg(test)]
mod tests {
    use std::process::{Command, Output};

    fn invoke(args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(args)
            .output()
            .unwrap()
    }

    #[test]
    fn no_subcommand_returns_usage_exit_code() {
        let output = invoke(&[]);
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .starts_with("usage: cargo xtask <")
        );
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn unknown_subcommand_returns_usage_exit_code() {
        let output = invoke(&["unknown"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .starts_with("usage: cargo xtask <")
        );
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn size_rejects_extra_arguments() {
        let output = invoke(&["size", "extra"]);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stderr, b"usage: cargo xtask size\n");
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn binsize_requires_a_path() {
        let output = invoke(&["binsize"]);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stderr, b"usage: cargo xtask binsize <path>\n");
        assert!(output.stdout.is_empty());
    }

    #[test]
    fn coverage_rejects_extra_arguments() {
        let output = invoke(&["coverage", "extra"]);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stderr, b"usage: cargo xtask coverage\n");
        assert!(output.stdout.is_empty());
    }
}
