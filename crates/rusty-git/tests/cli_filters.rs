#[cfg(test)]
mod fixture;

#[cfg(test)]
mod cli_filters {
    use std::ffi::OsString;
    use std::fs;
    use std::io::{Read, Write};
    use std::sync::{Mutex, MutexGuard};

    use rusty_git::{
        CliGit, CliOptions, FilterNotice, GitPath, GitSource, OtherPath, Others, TreeKind,
    };

    use super::fixture::Fixture;

    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn fixture() -> Option<Fixture> {
        match Fixture::new() {
            Ok(fixture) => Some(fixture),
            Err(reason) => {
                let mut stderr = std::io::stderr();
                writeln!(stderr, "skipping: {reason}").unwrap_or_default();
                None
            }
        }
    }

    fn other<'a>(paths: &'a [OtherPath], path: &[u8]) -> &'a OtherPath {
        paths
            .iter()
            .find(|entry| entry.path.as_bytes() == path)
            .unwrap_or_else(|| panic!("missing path {:?}", String::from_utf8_lossy(path)))
    }

    fn read_form(cli: &CliGit, path: &str) -> Vec<u8> {
        let mut form = cli
            .read_blob_worktree_form("HEAD", &GitPath::from(path))
            .unwrap();
        assert!(form.reason.is_none());
        let mut bytes = Vec::new();
        form.reader.read_to_end(&mut bytes).unwrap();
        bytes
    }

    fn autocrlf_form_case(autocrlf: &str, attribute: &str) {
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.git_ok(&["config", "core.autocrlf", autocrlf]);
        fixture.write(".gitattributes", format!("*.txt {attribute}\n").as_bytes());
        fixture.write("file.txt", b"one\ntwo\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("one");
        fs::remove_file(fixture.repo().join("file.txt")).unwrap();
        fixture.git_ok(&["checkout", "--", "file.txt"]);
        let checked_out = fs::read(fixture.repo().join("file.txt")).unwrap();
        let cli = CliGit::new(fixture.repo()).unwrap();

        assert_eq!(
            read_form(&cli, "file.txt"),
            checked_out,
            "worktree form differs for core.autocrlf={autocrlf} {attribute}"
        );
        let mut trait_blob = cli
            .read_blob_worktree_form("HEAD", &GitPath::from("file.txt"))
            .unwrap();
        let mut trait_bytes = Vec::new();
        trait_blob.reader.read_to_end(&mut trait_bytes).unwrap();
        assert_eq!(trait_bytes, checked_out);
    }

    #[test]
    fn autocrlf_worktree_form_matrix() {
        let _guard = serial();
        for autocrlf in ["true", "input", "false"] {
            for attribute in ["text", "eol=crlf", "-text"] {
                autocrlf_form_case(autocrlf, attribute);
            }
        }
    }

    #[test]
    fn worktree_form_keeps_unusual_paths() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write(".gitattributes", b"*.txt text eol=crlf\n");
        fixture.write("space name.txt", b"one\ntwo\n");
        fixture.write("unicode-\u{00e9}-\u{65e5}.txt", b"one\ntwo\n");
        #[cfg(not(windows))]
        fixture.write("line\nbreak.txt", b"one\ntwo\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("one");
        let cli = CliGit::new(fixture.repo()).unwrap();

        #[cfg(not(windows))]
        let paths = [
            "space name.txt",
            "unicode-\u{00e9}-\u{65e5}.txt",
            "line\nbreak.txt",
        ];
        #[cfg(windows)]
        let paths = ["space name.txt", "unicode-\u{00e9}-\u{65e5}.txt"];
        for path in paths {
            fs::remove_file(fixture.repo().join(path)).unwrap();
            fixture.git_ok(&["checkout", "--", path]);
            let checked_out = fs::read(fixture.repo().join(path)).unwrap();
            assert!(checked_out.windows(2).any(|window| window == b"\r\n"));
            assert_eq!(read_form(&cli, path), checked_out, "path {path:?}");
        }
    }

    #[test]
    fn untracked_and_ignored_lists_differ() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write(".gitignore", b"ignored.txt\nbuild/\n*.log\n");
        fixture.write("tracked.txt", b"tracked");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("one");
        fixture.write("space name.txt", b"untracked");
        fixture.write("unicode-\u{00e9}.txt", b"untracked");
        fixture.write("ignored.txt", b"ignored");
        fixture.write("build/out.bin", b"ignored");
        fixture.write("debug.log", b"ignored");
        #[cfg(not(windows))]
        fixture.write("line\nbreak.txt", b"untracked");
        let cli = CliGit::new(fixture.repo()).unwrap();

        let untracked = cli.others(Others::Untracked).unwrap();
        for path in [
            "space name.txt".as_bytes(),
            "unicode-\u{00e9}.txt".as_bytes(),
        ] {
            assert!(!other(&untracked, path).is_folder);
        }
        #[cfg(not(windows))]
        other(&untracked, b"line\nbreak.txt");
        assert!(
            untracked
                .iter()
                .all(|entry| entry.path.as_bytes() != b"ignored.txt")
        );
        assert!(
            untracked
                .iter()
                .all(|entry| entry.path.as_bytes() != b"build")
        );

        let ignored = cli.others(Others::Ignored).unwrap();
        assert!(!other(&ignored, b"ignored.txt").is_folder);
        assert!(!other(&ignored, b"debug.log").is_folder);
        assert!(other(&ignored, b"build").is_folder);
        assert!(
            ignored
                .iter()
                .all(|entry| entry.path.as_bytes() != b"space name.txt")
        );
    }

    #[test]
    fn index_entries_keep_their_kinds() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("file.txt", b"file\n");
        fixture.git_ok(&["add", "file.txt"]);
        let output = fixture.git_stdin_ok(&["hash-object", "-w", "--stdin"], b"file.txt");
        let oid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        fixture.git_ok(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("120000,{oid},link"),
        ]);
        let cli = CliGit::new(fixture.repo()).unwrap();
        let entries = cli.index_entries(rusty_git::Stage::Base).unwrap();
        let file = entries
            .iter()
            .find(|entry| entry.path.as_bytes() == b"file.txt")
            .unwrap();
        assert_eq!(file.kind, TreeKind::File);
        let link = entries
            .iter()
            .find(|entry| entry.path.as_bytes() == b"link")
            .unwrap();
        assert_eq!(link.kind, TreeKind::Link);
    }

    fn lfs_missing_options() -> CliOptions {
        CliOptions {
            env: vec![
                (OsString::from("GIT_CONFIG_COUNT"), OsString::from("3")),
                (
                    OsString::from("GIT_CONFIG_KEY_0"),
                    OsString::from("filter.lfs.process"),
                ),
                (OsString::from("GIT_CONFIG_VALUE_0"), OsString::new()),
                (
                    OsString::from("GIT_CONFIG_KEY_1"),
                    OsString::from("filter.lfs.smudge"),
                ),
                (
                    OsString::from("GIT_CONFIG_VALUE_1"),
                    OsString::from("git-lfs-not-installed-xyz"),
                ),
                (
                    OsString::from("GIT_CONFIG_KEY_2"),
                    OsString::from("filter.lfs.required"),
                ),
                (OsString::from("GIT_CONFIG_VALUE_2"), OsString::from("true")),
            ],
            ..CliOptions::default()
        }
    }

    fn lfs_blob(cli: &CliGit, path: &GitPath) -> (Vec<u8>, String) {
        let mut blob = cli.read_blob_worktree_form("HEAD", path).unwrap();
        let FilterNotice::Lfs { detail } = blob.reason.unwrap() else {
            panic!("the LFS pointer did not carry an LFS notice");
        };
        let mut bytes = Vec::new();
        blob.reader.read_to_end(&mut bytes).unwrap();
        (bytes, detail)
    }

    #[test]
    fn an_optional_filter_failure_is_a_notice() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write(".gitattributes", b"*.txt filter=custom -text\n");
        fixture.write("data.txt", b"raw payload\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("filter");
        fixture.git_ok(&[
            "config",
            "filter.custom.smudge",
            "git-filter-command-that-does-not-exist",
        ]);
        let cli = CliGit::new(fixture.repo()).unwrap();

        let mut blob = cli
            .read_blob_worktree_form("HEAD", &GitPath::from("data.txt"))
            .unwrap();
        let FilterNotice::Filter { detail } = blob.reason.unwrap() else {
            panic!("the optional filter failure did not carry a notice");
        };
        assert!(!detail.trim().is_empty());
        let mut bytes = Vec::new();
        blob.reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"raw payload\n");
    }

    #[test]
    fn an_lfs_pointer_is_returned_with_a_reason() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write(
            ".gitattributes",
            b"*.bin filter=lfs diff=lfs merge=lfs -text\n",
        );
        let pointer = b"version https://git-lfs.github.com/spec/v1\noid sha256:0123\nsize 12\n";
        fixture.write("data.bin", pointer);
        let output =
            fixture.git_stdin_ok(&["hash-object", "-w", "--no-filters", "--stdin"], pointer);
        let oid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        fixture.git_ok(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{oid},data.bin"),
        ]);
        fixture.git_ok(&["add", ".gitattributes"]);
        fixture.commit("lfs");
        let cli = CliGit::with_options(fixture.repo(), &lfs_missing_options()).unwrap();
        let path = GitPath::from("data.bin");

        let (bytes, detail) = lfs_blob(&cli, &path);
        assert!(
            detail.to_lowercase().contains("lfs"),
            "unexpected reason {detail:?}"
        );
        assert_eq!(bytes, pointer, "a failing LFS filter hid the pointer");
    }
}
