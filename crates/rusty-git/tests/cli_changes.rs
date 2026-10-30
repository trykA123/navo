#[cfg(test)]
mod fixture;

#[cfg(test)]
mod cli_changes {
    use std::fs;
    use std::io::{Read, Write};
    use std::sync::{Mutex, MutexGuard};

    use rusty_git::{Change, ChangeStatus, CliGit, GitSource, ObjectId, Stage, Target, TreeKind};

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

    fn change<'a>(changes: &'a [Change], path: &[u8]) -> &'a Change {
        changes
            .iter()
            .find(|change| change.path.as_bytes() == path)
            .unwrap_or_else(|| panic!("missing change {:?}", String::from_utf8_lossy(path)))
    }

    fn read_blob(cli: &CliGit, id: &ObjectId) -> Vec<u8> {
        let mut bytes = Vec::new();
        cli.read_blob(id).unwrap().read_to_end(&mut bytes).unwrap();
        bytes
    }

    fn stage_blob(cli: &CliGit, stage: Stage, path: &[u8]) -> Vec<u8> {
        let entry = cli
            .index_entries(stage)
            .unwrap()
            .into_iter()
            .find(|entry| entry.path.as_bytes() == path)
            .unwrap_or_else(|| {
                panic!(
                    "missing {stage:?} entry {:?}",
                    String::from_utf8_lossy(path)
                )
            });
        read_blob(cli, &entry.id)
    }

    #[test]
    fn renames_are_a_delete_and_an_add() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("old name.txt", b"content");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("one");
        fixture.git_ok(&["mv", "old name.txt", "new name.txt"]);
        fixture.commit("two");
        let cli = CliGit::new(fixture.repo()).unwrap();

        let changes = cli.changes("HEAD~1", Target::Rev("HEAD")).unwrap();
        assert_eq!(changes.len(), 2);
        let deleted = change(&changes, b"old name.txt");
        assert_eq!(deleted.status, ChangeStatus::Deleted);
        assert!(deleted.old_id.is_some());
        assert!(deleted.new_id.is_none());
        let added = change(&changes, b"new name.txt");
        assert_eq!(added.status, ChangeStatus::Added);
        assert!(added.old_id.is_none());
        assert!(added.new_id.is_some());
    }

    #[test]
    fn changes_follow_the_index_and_the_worktree() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("tracked.txt", b"one\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("one");
        fixture.write("tracked.txt", b"two\n");
        fixture.write("untracked.txt", b"untracked");
        let cli = CliGit::new(fixture.repo()).unwrap();

        let changes = cli.changes("HEAD", Target::Worktree).unwrap();
        assert_eq!(changes.len(), 1);
        let modified = change(&changes, b"tracked.txt");
        assert_eq!(modified.status, ChangeStatus::Modified);
        assert!(modified.old_id.is_some());
        assert!(modified.new_id.is_none());

        fixture.git_ok(&["add", "tracked.txt"]);
        let staged = cli.changes("HEAD", Target::Index).unwrap();
        assert_eq!(staged.len(), 1);
        let modified = change(&staged, b"tracked.txt");
        assert_eq!(modified.status, ChangeStatus::Modified);
        assert!(modified.old_id.is_some());
        assert!(modified.new_id.is_some());
        let worktree = cli.changes("HEAD", Target::Worktree).unwrap();
        assert_eq!(worktree.len(), 1);
        assert_eq!(
            change(&worktree, b"tracked.txt").status,
            ChangeStatus::Modified
        );
        assert!(change(&worktree, b"tracked.txt").new_id.is_some());
    }

    #[test]
    fn paths_with_spaces_unicode_and_newlines_round_trip() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("space name.txt", b"one");
        fixture.write("unicode-\u{00e9}-\u{65e5}.txt", b"one");
        #[cfg(not(windows))]
        fixture.write("line\nbreak.txt", b"one");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("one");
        fixture.write("space name.txt", b"two");
        fixture.write("unicode-\u{00e9}-\u{65e5}.txt", b"two");
        #[cfg(not(windows))]
        fixture.write("line\nbreak.txt", b"two");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("two");
        let cli = CliGit::new(fixture.repo()).unwrap();

        let changes = cli.changes("HEAD~1", Target::Rev("HEAD")).unwrap();
        for path in [
            "space name.txt".as_bytes(),
            "unicode-\u{00e9}-\u{65e5}.txt".as_bytes(),
        ] {
            assert_eq!(change(&changes, path).status, ChangeStatus::Modified);
        }
        #[cfg(not(windows))]
        assert_eq!(
            change(&changes, b"line\nbreak.txt").status,
            ChangeStatus::Modified
        );
    }

    fn conflicted_fixture() -> Option<Fixture> {
        let fixture = fixture()?;
        fixture.init(&["-b", "main"]);
        fixture.write("normal.txt", b"normal\n");
        fixture.write("shared.txt", b"base\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("base");
        fixture.git_ok(&["checkout", "-b", "other"]);
        fixture.write("shared.txt", b"theirs\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("theirs");
        fixture.git_ok(&["checkout", "main"]);
        fixture.write("shared.txt", b"ours\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("ours");
        fixture.git(&["merge", "other"]);
        Some(fixture)
    }

    #[test]
    fn a_conflict_carries_its_stages_and_stage_blobs() {
        let _guard = serial();
        let Some(fixture) = conflicted_fixture() else {
            return;
        };
        let cli = CliGit::new(fixture.repo()).unwrap();

        let changes = cli.changes("HEAD", Target::Index).unwrap();
        let conflict = change(&changes, b"shared.txt");
        let ChangeStatus::Conflict { stages } = conflict.status else {
            panic!("shared.txt was not a conflict: {:?}", conflict.status);
        };
        assert!(stages.has(Stage::Base));
        assert!(stages.has(Stage::Ours));
        assert!(stages.has(Stage::Theirs));
        assert!(conflict.old_id.is_none());
        assert!(conflict.new_id.is_none());

        assert_eq!(stage_blob(&cli, Stage::Base, b"shared.txt"), b"base\n");
        assert_eq!(stage_blob(&cli, Stage::Ours, b"shared.txt"), b"ours\n");
        assert_eq!(stage_blob(&cli, Stage::Theirs, b"shared.txt"), b"theirs\n");

        let normal = cli.index_entries(Stage::Base).unwrap();
        let normal = normal
            .iter()
            .find(|entry| entry.path.as_bytes() == b"normal.txt")
            .unwrap();
        assert_eq!(normal.kind, TreeKind::File);
        assert_eq!(read_blob(&cli, &normal.id), b"normal\n");
    }

    #[test]
    fn worktree_conflicts_use_the_index_stages() {
        let _guard = serial();
        let Some(fixture) = conflicted_fixture() else {
            return;
        };
        let cli = CliGit::new(fixture.repo()).unwrap();

        let changes = cli.changes("HEAD", Target::Worktree).unwrap();
        let conflict = change(&changes, b"shared.txt");
        let ChangeStatus::Conflict { stages } = conflict.status else {
            panic!(
                "worktree shared.txt was not a conflict: {:?}",
                conflict.status
            );
        };
        assert!(stages.has(Stage::Base));
        assert!(stages.has(Stage::Ours));
        assert!(stages.has(Stage::Theirs));

        fixture.write("shared.txt", b"ours\n");
        let changes = cli.changes("HEAD", Target::Worktree).unwrap();
        let conflict = change(&changes, b"shared.txt");
        let ChangeStatus::Conflict { stages } = conflict.status else {
            panic!("vanished conflict: {:?}", conflict.status);
        };
        assert!(stages.has(Stage::Base));
        assert!(stages.has(Stage::Ours));
        assert!(stages.has(Stage::Theirs));
    }

    #[test]
    fn a_branch_and_file_with_the_same_name_are_clear() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("main", b"same\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("one");
        let cli = CliGit::new(fixture.repo()).unwrap();

        assert!(cli.changes("main", Target::Worktree).unwrap().is_empty());
        assert!(cli.changes("main", Target::Index).unwrap().is_empty());
        assert!(cli.changes("main", Target::Rev("main")).unwrap().is_empty());

        fixture.write("main", b"changed\n");
        let changes = cli.changes("main", Target::Worktree).unwrap();
        assert_eq!(change(&changes, b"main").status, ChangeStatus::Modified);
    }

    #[test]
    fn worktree_changes_leave_the_index_untouched() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("tracked.txt", b"one\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("one");
        fixture.write("tracked.txt", b"dirty\n");
        fixture.write("untracked.txt", b"new");
        let index = fixture.repo().join(".git/index");
        let before = fs::metadata(&index).unwrap().modified().unwrap();
        let bytes_before = fs::read(&index).unwrap();
        let cli = CliGit::new(fixture.repo()).unwrap();

        let changes = cli.changes("HEAD", Target::Worktree).unwrap();
        assert_eq!(
            change(&changes, b"tracked.txt").status,
            ChangeStatus::Modified
        );

        let after = fs::metadata(&index).unwrap().modified().unwrap();
        assert_eq!(
            after, before,
            "changes(Target::Worktree) rewrote .git/index"
        );
        assert_eq!(
            fs::read(&index).unwrap(),
            bytes_before,
            "changes(Target::Worktree) changed .git/index bytes"
        );
    }

    fn autocrlf_changes_case(autocrlf: &str, attribute: &str) {
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
        let cli = CliGit::new(fixture.repo()).unwrap();
        assert!(
            cli.changes("HEAD", Target::Worktree).unwrap().is_empty(),
            "checked out content is not clean for core.autocrlf={autocrlf} {attribute}"
        );

        let replacement: &[u8] = if attribute == "-text" {
            b"one\r\ntwo\r\n"
        } else {
            b"one\ntwo\n"
        };
        fixture.write("file.txt", replacement);
        let changes = cli.changes("HEAD", Target::Worktree).unwrap();
        if attribute == "-text" {
            assert_eq!(
                change(&changes, b"file.txt").status,
                ChangeStatus::Modified,
                "untracked line endings were filtered for core.autocrlf={autocrlf} {attribute}"
            );
        } else {
            assert!(
                changes.is_empty(),
                "line endings were not filtered for core.autocrlf={autocrlf} {attribute}"
            );
        }
    }

    #[test]
    fn autocrlf_worktree_changes_matrix() {
        let _guard = serial();
        for autocrlf in ["true", "input", "false"] {
            for attribute in ["text", "eol=crlf", "-text"] {
                autocrlf_changes_case(autocrlf, attribute);
            }
        }
    }

    #[test]
    fn an_empty_comparison_is_clean() {
        let _guard = serial();
        let Some(fixture) = fixture() else {
            return;
        };
        fixture.init(&["-b", "main"]);
        fixture.write("file.txt", b"same\n");
        fixture.git_ok(&["add", "-A"]);
        fixture.commit("one");
        let cli = CliGit::new(fixture.repo()).unwrap();
        assert!(cli.changes("HEAD", Target::Rev("HEAD")).unwrap().is_empty());
        assert!(cli.changes("HEAD", Target::Worktree).unwrap().is_empty());
        assert!(cli.changes("HEAD", Target::Index).unwrap().is_empty());
    }
}
