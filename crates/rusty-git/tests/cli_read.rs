#[cfg(test)]
mod fixture;

#[cfg(test)]
mod cli_read {
    use std::ffi::OsString;
    use std::io::{Read, Write};
    use std::sync::{Mutex, MutexGuard};
    use std::time::Duration;

    use rusty_git::{CliGit, CliOptions, GitError, GitSource, ObjectId, RefKind, TreeKind};

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

    fn paths_repo(fixture: &Fixture) {
        fixture.init(&["-b", "main"]);
        fixture.write("space name.txt", b"space");
        fixture.write("unicode-\u{00e9}-\u{65e5}.txt", b"unicode");
        fixture.git_ok(&["add", "-A"]);
        #[cfg(not(windows))]
        fixture.add_blob("line\nbreak.txt", b"line\nbreak");
        fixture.commit("fixture");
    }

    #[test]
    fn sha1_reads_refs_tree_and_blobs() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        paths_repo(&fixture);
        let cli = CliGit::new(fixture.repo()).unwrap();
        let head = cli.resolve("HEAD").unwrap().unwrap();
        assert!(matches!(head, ObjectId::Sha1(_)));
        assert!(cli.resolve("does-not-exist").unwrap().is_none());
        assert_eq!(cli.merge_base("HEAD", "HEAD").unwrap(), Some(head));

        fixture.git_ok(&["update-ref", "refs/heads/-x", "HEAD"]);
        assert_eq!(cli.resolve("-x").unwrap(), Some(head));
        let refs = cli.refs().unwrap();
        assert!(refs.iter().any(|reference| {
            reference.name == "refs/heads/-x" && reference.kind == RefKind::Branch
        }));
        assert!(
            refs.iter()
                .any(|reference| reference.name == "refs/heads/main")
        );

        let entries = cli.list_tree("HEAD").unwrap();
        let space = entry(&entries, b"space name.txt");
        assert_eq!(space.kind, TreeKind::File);
        assert_eq!(space.size, Some(5));
        entry(&entries, "unicode-\u{00e9}-\u{65e5}.txt".as_bytes());
        assert_eq!(read(&cli, &space.id), b"space");
        #[cfg(not(windows))]
        {
            assert_eq!(entry(&entries, b"line\nbreak.txt").size, Some(10));
            let newline = entry(&entries, b"line\nbreak.txt");
            assert_eq!(read(&cli, &newline.id), b"line\nbreak");
        }
    }

    fn entry<'a>(entries: &'a [rusty_git::TreeEntry], path: &[u8]) -> &'a rusty_git::TreeEntry {
        entries
            .iter()
            .find(|entry| entry.path.as_bytes() == path)
            .unwrap_or_else(|| panic!("missing tree entry {:?}", String::from_utf8_lossy(path)))
    }

    fn read(cli: &CliGit, id: &ObjectId) -> Vec<u8> {
        let mut bytes = Vec::new();
        cli.read_blob(id).unwrap().read_to_end(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn sha256_reftable_repository_is_read() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        let init = fixture.try_init(&[
            "-b",
            "main",
            "--object-format=sha256",
            "--ref-format=reftable",
        ]);
        if !init.status.success() {
            let mut stderr = std::io::stderr();
            writeln!(
                stderr,
                "skipping sha256/reftable: {}",
                String::from_utf8_lossy(&init.stderr).trim()
            )
            .unwrap_or_default();
            return;
        }
        fixture.write("file.txt", b"sha256");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("sha256");
        let cli = CliGit::new(fixture.repo()).unwrap();
        let head = cli.resolve("HEAD").unwrap().unwrap();
        assert!(matches!(head, ObjectId::Sha256(_)));
        let entries = cli.list_tree("HEAD").unwrap();
        let file = entry(&entries, b"file.txt");
        assert_eq!(read(&cli, &file.id), b"sha256");
    }

    #[test]
    fn a_missing_object_is_a_typed_error() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        let cli = CliGit::new(fixture.repo()).unwrap();
        let missing = ObjectId::Sha1([0xFF; 20]);
        assert!(matches!(
            cli.read_blob(&missing),
            Err(GitError::MissingObject { .. })
        ));
    }

    #[test]
    fn a_large_blob_is_streamed() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        let data = vec![0x5A; 8 * 1024 * 1024];
        fixture.add_blob("large.bin", &data);
        fixture.commit("large");
        let cli = CliGit::new(fixture.repo()).unwrap();
        let entries = cli.list_tree("HEAD").unwrap();
        let large = entry(&entries, b"large.bin");
        assert_eq!(large.size, Some(data.len() as u64));
        let mut reader = cli.read_blob(&large.id).unwrap();
        let mut buffer = [0u8; 64 * 1024];
        let mut offset = 0;
        loop {
            let read = reader.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            assert_eq!(&buffer[..read], &data[offset..offset + read]);
            offset += read;
        }
        assert_eq!(offset, data.len());
    }

    #[test]
    fn partial_blob_drops_keep_or_restart_the_session() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        let small = vec![0x11; 1024];
        let large = vec![0x22; 2 * 1024 * 1024];
        fixture.add_blob("small.bin", &small);
        fixture.add_blob("large.bin", &large);
        fixture.commit("partial drops");
        let cli = CliGit::new(fixture.repo()).unwrap();
        let entries = cli.list_tree("HEAD").unwrap();
        let small_id = &entry(&entries, b"small.bin").id;
        let large_id = &entry(&entries, b"large.bin").id;

        let mut reader = cli.read_blob(small_id).unwrap();
        let mut first = [0u8; 1];
        reader.read_exact(&mut first).unwrap();
        drop(reader);
        assert_eq!(read(&cli, small_id), small);

        let mut reader = cli.read_blob(large_id).unwrap();
        reader.read_exact(&mut first).unwrap();
        drop(reader);
        assert_eq!(read(&cli, large_id), large);
    }

    #[test]
    fn an_empty_blob_reader_disarms_the_watchdog() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.add_blob("empty.bin", b"");
        fixture.add_blob("next.bin", b"next");
        fixture.commit("empty");
        let cli = CliGit::with_timeout(fixture.repo(), Duration::from_millis(100)).unwrap();
        let entries = cli.list_tree("HEAD").unwrap();
        let empty = cli.read_blob(&entry(&entries, b"empty.bin").id).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(read(&cli, &entry(&entries, b"next.bin").id), b"next");
        drop(empty);
    }

    #[test]
    fn an_old_reader_does_not_abort_the_new_session() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        let large = vec![0x33; 2 * 1024 * 1024];
        fixture.add_blob("large.bin", &large);
        fixture.commit("generation");
        let cli = CliGit::with_timeout(fixture.repo(), Duration::from_millis(100)).unwrap();
        let entries = cli.list_tree("HEAD").unwrap();
        let id = &entry(&entries, b"large.bin").id;
        let mut old = cli.read_blob(id).unwrap();
        let mut buffer = [0u8; 8192];
        old.read_exact(&mut buffer[..1]).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let timed_out = loop {
            match old.read(&mut buffer) {
                Ok(0) => break false,
                Ok(_) => {}
                Err(error) => break error.kind() == std::io::ErrorKind::TimedOut,
            }
        };
        assert!(timed_out);
        let mut new = cli.read_blob(id).unwrap();
        drop(old);
        let mut bytes = Vec::new();
        new.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, large);
    }

    #[cfg(unix)]
    #[test]
    fn a_wrapper_that_keeps_the_pipes_open_still_times_out() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        let git = fixture.script(
            "git-pipes",
            "#!/bin/sh\n\
             case \"$*\" in\n\
             *--version*) echo 'git version 2.56.0' ;;\n\
             *rev-parse*--git-dir*) echo .git ;;\n\
             *ls-tree*) (sleep 2) & echo started; exit 0 ;;\n\
             *) exit 1 ;;\n\
             esac\n",
        );
        let options = CliOptions {
            timeout: Duration::from_millis(100),
            git: Some(git),
            ..CliOptions::default()
        };
        let cli = CliGit::with_options(fixture.repo(), &options).unwrap();
        let started = std::time::Instant::now();
        assert!(matches!(cli.list_tree("HEAD"), Err(GitError::Timeout)));
        assert!(started.elapsed() < Duration::from_millis(400));
    }

    #[test]
    fn dubious_ownership_is_typed() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        let options = CliOptions {
            env: vec![(
                OsString::from("GIT_TEST_ASSUME_DIFFERENT_OWNER"),
                OsString::from("1"),
            )],
            ..CliOptions::default()
        };
        match CliGit::with_options(fixture.repo(), &options) {
            Ok(_) => panic!("dubious ownership was accepted"),
            Err(GitError::UnsafeDirectory { path }) => {
                assert_eq!(path, fixture.repo());
            }
            Err(error) => panic!("unexpected error: {error}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn hung_commands_and_batches_time_out() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        let git = fixture.script(
            "git-hangs",
            "#!/bin/sh\n\
             case \"$*\" in\n\
             *--version*) echo 'git version 2.56.0' ;;\n\
             *rev-parse*--git-dir*) echo .git ;;\n\
             *ls-tree*) exec sleep 5 ;;\n\
             *cat-file*) exec sleep 5 ;;\n\
             *) echo unexpected >&2; exit 1 ;;\n\
             esac\n",
        );
        let options = CliOptions {
            timeout: Duration::from_millis(100),
            git: Some(git),
            ..CliOptions::default()
        };
        let cli = CliGit::with_options(fixture.repo(), &options).unwrap();
        assert!(matches!(cli.list_tree("HEAD"), Err(GitError::Timeout)));
        let missing = ObjectId::Sha1([0xAB; 20]);
        assert!(matches!(cli.read_blob(&missing), Err(GitError::Timeout)));
        assert!(matches!(cli.read_blob(&missing), Err(GitError::Timeout)));
    }

    #[cfg(unix)]
    #[test]
    fn a_batch_failure_keeps_its_stderr() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        let git = fixture.script(
            "git-cat-file-fails",
            "#!/bin/sh\n\
             case \"$*\" in\n\
             *--version*) echo 'git version 2.56.0' ;;\n\
             *rev-parse*--git-dir*) echo .git ;;\n\
             *cat-file*) echo 'cat-file exploded' >&2; exit 1 ;;\n\
             *) echo unexpected >&2; exit 1 ;;\n\
             esac\n",
        );
        let options = CliOptions {
            git: Some(git),
            ..CliOptions::default()
        };
        let cli = CliGit::with_options(fixture.repo(), &options).unwrap();
        let missing = ObjectId::Sha1([0xCD; 20]);
        match cli.read_blob(&missing) {
            Err(GitError::Command { args, stderr, .. }) => {
                assert!(args.contains("cat-file"));
                assert!(stderr.contains("cat-file exploded"));
            }
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("the batch failure was not reported"),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dropping_the_session_reaps_cat_file() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        let before = cat_file_children().len();
        let cli = CliGit::new(fixture.repo()).unwrap();
        let missing = ObjectId::Sha1([0xFF; 20]);
        assert!(matches!(
            cli.read_blob(&missing),
            Err(GitError::MissingObject { .. })
        ));
        assert!(cat_file_children().len() > before);
        drop(cli);
        for _ in 0..50 {
            if cat_file_children().len() <= before {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("the cat-file child was not reaped");
    }

    #[cfg(target_os = "linux")]
    fn cat_file_children() -> Vec<u32> {
        let parent = std::process::id();
        let mut children = Vec::new();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return children;
        };
        for entry in entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            let Some(close) = stat.rfind(')') else {
                continue;
            };
            let mut fields = stat[close + 1..].split_whitespace();
            let _state = fields.next();
            let ppid = fields.next().and_then(|value| value.parse::<u32>().ok());
            if ppid != Some(parent) {
                continue;
            }
            let cmdline = std::fs::read(entry.path().join("cmdline")).unwrap_or_default();
            if cmdline.windows(8).any(|window| window == b"cat-file") {
                children.push(pid);
            }
        }
        children
    }
}
