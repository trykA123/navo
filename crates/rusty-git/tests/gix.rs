#[cfg(test)]
mod fixture;

#[cfg(test)]
mod gix {
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard};
    use std::time::SystemTime;

    use rusty_git::{
        CliGit, GitError, GitPath, GitSource, GixGit, ObjectId, Others, Stage, Target, TreeKind,
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

    fn read(mut reader: rusty_git::BlobReader) -> Vec<u8> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    }

    fn oid(hex: &str) -> ObjectId {
        let bytes: Vec<u8> = hex
            .as_bytes()
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        ObjectId::from_bytes(&bytes).unwrap()
    }

    fn snapshot(root: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
        let mut files = Vec::new();
        collect_files(root, root, &mut files);
        files.sort_by(|left, right| left.0.cmp(&right.0));
        files
    }

    fn collect_files(root: &Path, folder: &Path, files: &mut Vec<(PathBuf, u64, SystemTime)>) {
        for entry in std::fs::read_dir(folder).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                collect_files(root, &path, files);
            } else {
                files.push((
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    metadata.len(),
                    metadata.modified().unwrap(),
                ));
            }
        }
    }

    fn assert_read_only(fixture: &Fixture, operation: impl FnOnce()) {
        let git = fixture.repo().join(".git");
        let before = snapshot(&git);
        operation();
        assert_eq!(snapshot(&git), before);
    }

    fn write_ref(fixture: &Fixture, name: &str, id: &str) {
        fixture.write(&format!(".git/{name}"), format!("{id}\n").as_bytes());
    }

    fn hash_literal(fixture: &Fixture, kind: &str, bytes: &[u8]) -> String {
        let output = fixture.git_stdin_ok(
            &["hash-object", "--literally", "-t", kind, "-w", "--stdin"],
            bytes,
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn rev_parse(fixture: &Fixture, name: &str) -> String {
        let output = fixture.git_ok(&["rev-parse", name]);
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn tree_fixture(fixture: &Fixture) {
        fixture.init(&["-b", "main"]);
        fixture.write("a.txt", b"base\n");
        fixture.write("dir/nested.txt", b"nested\n");
        fixture.write("space name.txt", b"space\n");
        fixture.write("unicode-\u{00e9}.txt", b"unicode\n");
        fixture.write("tool.sh", b"#!/bin/sh\n");
        #[cfg(not(windows))]
        fixture.write("line\nbreak.txt", b"newline\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.git_ok(&["update-index", "--chmod=+x", "tool.sh"]);
        let output = fixture.git_stdin_ok(&["hash-object", "-w", "--stdin"], b"a.txt");
        let oid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        fixture.git_ok(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("120000,{oid},link"),
        ]);
        fixture.git_ok(&[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,1111111111111111111111111111111111111111,sub",
        ]);
        fixture.commit("base");
        fixture.git_ok(&["branch", "other"]);
        fixture.write("a.txt", b"main\n");
        fixture.write("main-only.txt", b"main\n");
        fixture.git_ok(&["add", "a.txt", "main-only.txt"]);
        fixture.commit("main");
    }

    fn refs_fixture(fixture: &Fixture) {
        fixture.git_ok(&["tag", "light"]);
        fixture.git_ok(&["tag", "-a", "release", "-m", "release"]);
        let blob = String::from_utf8_lossy(&fixture.git_ok(&["rev-parse", "HEAD:a.txt"]).stdout)
            .trim()
            .to_owned();
        fixture.git_ok(&["tag", "blobtag", &blob]);
        let tree = String::from_utf8_lossy(&fixture.git_ok(&["rev-parse", "HEAD^{tree}"]).stdout)
            .trim()
            .to_owned();
        fixture.git_ok(&["tag", "treetag", &tree]);
        fixture.git_ok(&["update-ref", "refs/heads/-x", "HEAD"]);
        fixture.git_ok(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/tags/release",
        ]);
        fixture.git_ok(&[
            "symbolic-ref",
            "refs/remotes/origin/dangling",
            "refs/heads/missing",
        ]);
        fixture.git_ok(&["pack-refs", "--all"]);
    }

    #[test]
    fn gix_and_cli_agree_on_a_fixture() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        tree_fixture(&fixture);
        refs_fixture(&fixture);
        let cli = CliGit::new(fixture.repo()).unwrap();
        let gix = GixGit::new(fixture.repo()).unwrap();
        assert_trees(&gix, &cli);
        assert_refs_and_revisions(&gix, &cli);
    }

    fn assert_trees(gix: &GixGit, cli: &CliGit) {
        let cli_tree = cli.list_tree("main").unwrap();
        let gix_tree = gix.list_tree("main").unwrap();
        assert_eq!(gix_tree, cli_tree);
        let link = gix_tree
            .iter()
            .find(|entry| entry.path.as_bytes() == b"link")
            .unwrap();
        assert_eq!(link.kind, TreeKind::Link);
        let submodule = gix_tree
            .iter()
            .find(|entry| entry.path.as_bytes() == b"sub")
            .unwrap();
        assert_eq!(submodule.kind, TreeKind::Submodule);
        assert_eq!(submodule.size, None);
        for entry in &cli_tree {
            assert!(entry.size.is_some() || entry.kind == TreeKind::Submodule);
            if matches!(entry.kind, TreeKind::File | TreeKind::Link) {
                assert_eq!(
                    read(gix.read_blob(&entry.id).unwrap()),
                    read(cli.read_blob(&entry.id).unwrap()),
                    "blob {}",
                    entry.path
                );
            }
        }
    }

    fn assert_refs_and_revisions(gix: &GixGit, cli: &CliGit) {
        let refs = gix.refs().unwrap();
        assert_eq!(refs, cli.refs().unwrap());
        let names: Vec<&str> = refs
            .iter()
            .map(|reference| reference.name.as_str())
            .collect();
        assert!(names.contains(&"refs/remotes/origin/HEAD"));
        assert!(names.contains(&"refs/tags/release"));
        assert!(!names.contains(&"refs/remotes/origin/dangling"));
        for name in [
            "main",
            "other",
            "HEAD",
            "light",
            "release",
            "-x",
            "blobtag",
            "treetag",
            "HEAD^{tree}",
            "HEAD..HEAD",
            "HEAD~100",
        ] {
            assert_eq!(
                gix.resolve(name).unwrap(),
                cli.resolve(name).unwrap(),
                "resolve {name}"
            );
        }
        assert_eq!(
            gix.resolve("does-not-exist").unwrap(),
            cli.resolve("does-not-exist").unwrap()
        );
        assert_eq!(
            gix.merge_base("main", "other").unwrap(),
            cli.merge_base("main", "other").unwrap()
        );
    }

    #[test]
    fn gix_reads_leave_the_repository_untouched() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        tree_fixture(&fixture);
        let gix = GixGit::new(fixture.repo()).unwrap();
        let tree = gix.list_tree("main").unwrap();
        let file = tree
            .iter()
            .find(|entry| entry.path.as_bytes() == b"a.txt")
            .unwrap();
        let id = file.id;

        assert_read_only(&fixture, || {
            gix.list_tree("main").unwrap();
        });
        assert_read_only(&fixture, || {
            gix.refs().unwrap();
        });
        assert_read_only(&fixture, || {
            gix.resolve("main").unwrap();
        });
        assert_read_only(&fixture, || {
            gix.merge_base("main", "other").unwrap();
        });
        assert_read_only(&fixture, || {
            read(gix.read_blob(&id).unwrap());
        });
    }

    #[test]
    fn a_missing_blob_is_typed() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        let gix = GixGit::new(fixture.repo()).unwrap();
        let missing = ObjectId::Sha1([0xAB; 20]);
        assert!(matches!(
            gix.read_blob(&missing),
            Err(GitError::MissingObject { .. })
        ));
    }

    #[test]
    fn an_absent_loose_object_is_absent_like_the_cli() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("a.txt", b"base\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("base");
        let head = rev_parse(&fixture, "HEAD");
        let object = fixture
            .repo()
            .join(".git/objects")
            .join(&head[..2])
            .join(&head[2..]);
        let cli = CliGit::new(fixture.repo()).unwrap();
        let gix = GixGit::new(fixture.repo()).unwrap();
        std::fs::remove_file(object).unwrap();

        let gix_result = gix.resolve("HEAD").unwrap();
        let cli_result = cli.resolve("HEAD").unwrap();
        assert_eq!(gix_result, cli_result);
        assert_eq!(gix_result, None);
    }

    #[test]
    fn a_missing_reflog_matches_the_cli() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("a.txt", b"base\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("base");
        std::fs::remove_dir_all(fixture.repo().join(".git/logs")).unwrap();
        let cli = CliGit::new(fixture.repo()).unwrap();
        let gix = GixGit::new(fixture.repo()).unwrap();

        let gix_result = gix.resolve("HEAD@{0}").unwrap();
        let cli_result = cli.resolve("HEAD@{0}").unwrap();
        assert_eq!(gix_result, cli_result);
        assert_eq!(gix_result, None);
    }

    #[test]
    fn corrupt_objects_do_not_panic() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("a.txt", b"base\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("base");

        let bad_tag = hash_literal(&fixture, "tag", b"garbage");
        let bad_commit = hash_literal(&fixture, "commit", b"invalid commit");
        let tag_on_bad_commit = hash_literal(
            &fixture,
            "tag",
            format!("object {bad_commit}\ntype commit\ntag corrupt\n").as_bytes(),
        );
        write_ref(&fixture, "refs/tags/badtag", &bad_tag);
        write_ref(&fixture, "refs/heads/badbranch", &bad_commit);
        write_ref(&fixture, "refs/tags/badcommit", &bad_commit);
        write_ref(&fixture, "refs/tags/corrupttag", &tag_on_bad_commit);

        let cli = CliGit::new(fixture.repo()).unwrap();
        let gix = GixGit::new(fixture.repo()).unwrap();

        assert!(gix.resolve("badtag").is_err());
        assert!(gix.list_tree("badtag").is_err());
        for name in ["badbranch", "badcommit", "corrupttag"] {
            let gix_result = gix.resolve(name).unwrap();
            let cli_result = cli.resolve(name).unwrap();
            assert_eq!(gix_result, cli_result, "resolve {name}");
            assert_eq!(gix_result, None, "resolve {name}");
            assert!(gix.list_tree(name).is_err(), "list_tree {name}");
        }
    }

    #[test]
    fn revision_failures_match_the_cli() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("a.txt", b"base\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("base");

        let cli = CliGit::new(fixture.repo()).unwrap();
        let gix = GixGit::new(fixture.repo()).unwrap();
        for name in ["HEAD@{99}", "HEAD@{upstream}"] {
            assert!(gix.resolve(name).is_err(), "gix {name}");
            assert!(cli.resolve(name).is_err(), "cli {name}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_loose_object_is_an_error() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("a.txt", b"base\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("base");
        let head = rev_parse(&fixture, "HEAD");
        let object = fixture
            .repo()
            .join(".git/objects")
            .join(&head[..2])
            .join(&head[2..]);

        let cli = CliGit::new(fixture.repo()).unwrap();
        let gix = GixGit::new(fixture.repo()).unwrap();
        let original = std::fs::metadata(&object).unwrap().permissions();
        std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o000)).unwrap();
        let gix_result = gix.resolve("HEAD");
        let cli_result = cli.resolve("HEAD");
        std::fs::set_permissions(&object, original).unwrap();

        assert!(gix_result.is_err());
        assert!(cli_result.is_err());
    }

    #[test]
    fn blobs_over_the_cap_are_refused() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        let path = fixture.repo().join("big.bin");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(256 * 1024 * 1024 + 1).unwrap();
        drop(file);
        let output = fixture.git_ok(&["hash-object", "-w", "big.bin"]);
        let hex = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let gix = GixGit::new(fixture.repo()).unwrap();
        assert!(matches!(
            gix.read_blob(&oid(&hex)),
            Err(GitError::Unsupported { .. })
        ));
    }

    #[test]
    fn unsupported_methods_are_typed() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        tree_fixture(&fixture);
        let gix = GixGit::new(fixture.repo()).unwrap();
        let path = GitPath::from("a.txt");
        assert!(matches!(
            gix.read_blob_worktree_form("main", &path),
            Err(GitError::Unsupported { .. })
        ));
        assert!(matches!(
            gix.changes("main", Target::Worktree),
            Err(GitError::Unsupported { .. })
        ));
        assert!(matches!(
            gix.index_entries(Stage::Base),
            Err(GitError::Unsupported { .. })
        ));
        assert!(matches!(
            gix.others(Others::Untracked),
            Err(GitError::Unsupported { .. })
        ));
        assert!(matches!(
            gix.changes("main", Target::Rev("HEAD")),
            Err(GitError::Unsupported { .. })
        ));
    }

    #[test]
    fn sha256_repositories_are_refused() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        let init = fixture.try_init(&["-b", "main", "--object-format=sha256"]);
        if !init.status.success() {
            skip_init(&init);
            return;
        }
        assert!(matches!(
            GixGit::new(fixture.repo()),
            Err(GitError::Unsupported { .. })
        ));
    }

    #[test]
    fn reftable_repositories_are_refused() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        let init = fixture.try_init(&["-b", "main", "--ref-format=reftable"]);
        if !init.status.success() {
            skip_init(&init);
            return;
        }
        assert!(matches!(
            GixGit::new(fixture.repo()),
            Err(GitError::Unsupported { .. })
        ));
    }

    fn skip_init(output: &std::process::Output) {
        let mut stderr = std::io::stderr();
        writeln!(
            stderr,
            "skipping: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .unwrap_or_default();
    }
}
