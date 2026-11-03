#[cfg(test)]
mod status_parse {
    use navo_git::status::{Badge, DirMap, Head, Operation, Reason, RepoStatus, Upstream};
    use navo_git::status::{badge, parse_porcelain_v2};
    use navo_git::{GitError, ObjectId};

    const OID: &str = "abababababababababababababababababababab";

    fn oid() -> ObjectId {
        ObjectId::from_bytes(&[0xab; 20]).expect("20 bytes")
    }

    fn record(parts: &[&str]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for part in parts {
            bytes.extend_from_slice(part.as_bytes());
            bytes.push(0);
        }
        bytes
    }

    fn branch_headers(extra: &[&str]) -> Vec<String> {
        let mut parts = vec![
            format!("# branch.oid {OID}"),
            "# branch.head main".to_owned(),
        ];
        parts.extend(extra.iter().map(|part| (*part).to_owned()));
        parts
    }

    fn parse(parts: &[String]) -> Result<RepoStatus, GitError> {
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        parse_porcelain_v2(&record(&refs), false)
    }

    fn status(changed: u32, untracked: u32, conflicted: u32) -> RepoStatus {
        RepoStatus {
            head: Head::Branch("main".to_owned()),
            upstream: None,
            changed,
            untracked,
            conflicted,
            operation: None,
            dirs: DirMap::default(),
            capped: false,
            fetched_ms: None,
        }
    }

    fn with_upstream(mut base: RepoStatus, ahead: u32, behind: u32) -> RepoStatus {
        base.upstream = Some(Upstream {
            name: "origin/main".to_owned(),
            ahead,
            behind,
        });
        base
    }

    #[test]
    fn clean_repository_has_no_counts() {
        let parts = branch_headers(&[]);
        assert_eq!(parse(&parts).expect("parses"), status(0, 0, 0));
    }

    #[test]
    fn staged_unstaged_and_untracked_are_counted_apart() {
        let mut parts = branch_headers(&[]);
        parts.push("1 M. N... 100644 100644 100644 aaaa bbbb staged.txt".to_owned());
        parts.push("1 .M N... 100644 100644 100644 aaaa aaaa edited.txt".to_owned());
        parts.push("1 MM N... 100644 100644 100644 aaaa bbbb both.txt".to_owned());
        parts.push("? new file.txt".to_owned());
        parts.push("! target/".to_owned());
        assert_eq!(parse(&parts).expect("parses"), status(3, 1, 0));
    }

    #[test]
    fn unmerged_records_count_as_conflicts() {
        let mut parts = branch_headers(&[]);
        parts.push("u UU N... 100644 100644 100644 100644 a b c clash.txt".to_owned());
        assert_eq!(parse(&parts).expect("parses"), status(0, 0, 1));
    }

    #[test]
    fn rename_record_skips_the_original_path() {
        let mut parts = branch_headers(&[]);
        parts.push("2 R. N... 100644 100644 100644 aaaa bbbb R100 new.txt".to_owned());
        parts.push("? old.txt".to_owned());
        parts.push("? other.txt".to_owned());
        assert_eq!(parse(&parts).expect("parses"), status(1, 1, 0));
    }

    #[test]
    fn detached_head_carries_the_commit() {
        let parts = vec![
            format!("# branch.oid {OID}"),
            "# branch.head (detached)".to_owned(),
        ];
        let mut expected = status(0, 0, 0);
        expected.head = Head::Detached(oid());
        assert_eq!(parse(&parts).expect("parses"), expected);
    }

    #[test]
    fn unborn_branch_keeps_its_name() {
        let parts = vec![
            "# branch.oid (initial)".to_owned(),
            "# branch.head main".to_owned(),
        ];
        let mut expected = status(0, 0, 0);
        expected.head = Head::Unborn("main".to_owned());
        assert_eq!(parse(&parts).expect("parses"), expected);
    }

    #[test]
    fn branch_without_upstream_has_none() {
        let parts = branch_headers(&[]);
        assert_eq!(parse(&parts).expect("parses").upstream, None);
    }

    #[test]
    fn ahead_and_behind_are_read_from_the_ab_header() {
        let parts = branch_headers(&["# branch.upstream origin/main", "# branch.ab +3 -2"]);
        let parsed = parse(&parts).expect("parses");
        assert_eq!(parsed, with_upstream(status(0, 0, 0), 3, 2));
    }

    #[test]
    fn upstream_without_ab_counts_zero() {
        let parts = branch_headers(&["# branch.upstream origin/gone"]);
        let upstream = parse(&parts).expect("parses").upstream.expect("upstream");
        assert_eq!((upstream.ahead, upstream.behind), (0, 0));
    }

    #[test]
    fn unknown_record_is_a_protocol_error() {
        let mut parts = branch_headers(&[]);
        parts.push("Z surprise".to_owned());
        assert!(matches!(parse(&parts), Err(GitError::Protocol { .. })));
    }

    #[test]
    fn malformed_ab_header_is_a_protocol_error() {
        let parts = branch_headers(&["# branch.upstream origin/main", "# branch.ab 3 2"]);
        assert!(matches!(parse(&parts), Err(GitError::Protocol { .. })));
    }

    #[test]
    fn missing_branch_headers_are_a_protocol_error() {
        assert!(matches!(
            parse_porcelain_v2(&record(&["? file.txt"]), false),
            Err(GitError::Protocol { .. })
        ));
    }

    fn truncated(capped: bool) -> Result<RepoStatus, GitError> {
        let parts = branch_headers(&["? one.txt"]);
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        let mut bytes = record(&refs);
        bytes.extend_from_slice(b"? two.tx");
        parse_porcelain_v2(&bytes, capped)
    }

    #[test]
    fn truncated_last_record_is_an_error_when_not_capped() {
        assert!(matches!(truncated(false), Err(GitError::Protocol { .. })));
    }

    #[test]
    fn truncated_last_record_is_counted_and_flagged_when_capped() {
        let parsed = truncated(true).expect("parses");
        assert_eq!((parsed.untracked, parsed.capped), (2, true));
    }

    #[test]
    fn garbage_last_record_is_ignored_when_capped() {
        let parts = branch_headers(&[]);
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        let mut bytes = record(&refs);
        bytes.push(b'1');
        let parsed = parse_porcelain_v2(&bytes, true).expect("parses");
        assert_eq!((parsed.changed, parsed.capped), (0, true));
    }

    #[test]
    fn badge_conflict_beats_everything() {
        let all = with_upstream(status(2, 1, 1), 1, 1);
        assert_eq!(badge(&all), Badge::Conflict);
    }

    #[test]
    fn badge_conflict_for_an_operation_in_progress() {
        for operation in [
            Operation::Merge,
            Operation::Rebase,
            Operation::CherryPick,
            Operation::Revert,
        ] {
            let mut busy = status(1, 0, 0);
            busy.operation = Some(operation);
            assert_eq!(badge(&busy), Badge::Conflict);
        }
    }

    #[test]
    fn badge_bisect_alone_is_not_a_conflict() {
        let mut busy = status(0, 0, 0);
        busy.operation = Some(Operation::Bisect);
        assert_eq!(badge(&busy), Badge::Clean);
    }

    #[test]
    fn badge_modified_beats_diverged() {
        assert_eq!(
            badge(&with_upstream(status(1, 0, 0), 2, 2)),
            Badge::Modified
        );
        assert_eq!(
            badge(&with_upstream(status(0, 1, 0), 2, 2)),
            Badge::Modified
        );
    }

    #[test]
    fn badge_diverged_needs_both_directions() {
        assert_eq!(
            badge(&with_upstream(status(0, 0, 0), 1, 1)),
            Badge::Diverged
        );
    }

    #[test]
    fn badge_ahead_then_behind_then_clean() {
        assert_eq!(badge(&with_upstream(status(0, 0, 0), 2, 0)), Badge::Ahead);
        assert_eq!(badge(&with_upstream(status(0, 0, 0), 0, 2)), Badge::Behind);
        assert_eq!(badge(&with_upstream(status(0, 0, 0), 0, 0)), Badge::Clean);
        assert_eq!(badge(&status(0, 0, 0)), Badge::Clean);
    }

    #[test]
    fn unknown_badge_carries_its_reason() {
        assert_ne!(
            Badge::Unknown(Reason::TimedOut),
            Badge::Unknown(Reason::NoGit)
        );
    }

    fn nested(capped: bool) -> RepoStatus {
        let mut parts = branch_headers(&["# branch.upstream origin/main", "# branch.ab +1 -0"]);
        parts.push("1 .M N... 100644 100644 100644 aaaa aaaa src/deep/er/lib.rs".to_owned());
        parts.push("? docs/notes/new file.md".to_owned());
        parts.push("u UU N... 100644 100644 100644 100644 a b c app/merge/clash.rs".to_owned());
        parts.push("? build/".to_owned());
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        parse_porcelain_v2(&record(&refs), capped).expect("parses")
    }

    #[test]
    fn deep_change_marks_every_folder_above_it() {
        let parsed = nested(false);
        for dir in ["src", "src/deep", "src/deep/er"] {
            assert_eq!(parsed.dir_badge(dir), Badge::Modified, "{dir}");
        }
    }

    #[test]
    fn untracked_file_and_folder_count_as_modified() {
        let parsed = nested(false);
        for dir in ["docs", "docs/notes", "build"] {
            assert_eq!(parsed.dir_badge(dir), Badge::Modified, "{dir}");
        }
    }

    #[test]
    fn conflict_in_a_sibling_subtree_stays_there() {
        let parsed = nested(false);
        assert_eq!(parsed.dir_badge("app"), Badge::Conflict);
        assert_eq!(parsed.dir_badge("app/merge"), Badge::Conflict);
        assert_eq!(parsed.dir_badge("src"), Badge::Modified);
    }

    #[test]
    fn clean_sibling_and_missing_folder_are_clean() {
        let parsed = nested(false);
        assert_eq!(parsed.dir_badge("tests"), Badge::Clean);
        assert_eq!(parsed.dir_badge("src/other"), Badge::Clean);
    }

    #[test]
    fn root_badge_is_the_worst_below_and_keeps_ahead_behind() {
        let parsed = nested(false);
        assert_eq!(parsed.dir_badge(""), Badge::Conflict);
        assert_eq!(parsed.upstream.expect("upstream").ahead, 1);
    }

    #[test]
    fn capped_status_makes_subfolders_unknown_but_not_the_root() {
        let parsed = nested(true);
        assert_eq!(parsed.dir_badge("src"), Badge::Unknown(Reason::TooLarge));
        assert_eq!(parsed.dir_badge("tests"), Badge::Unknown(Reason::TooLarge));
        assert_eq!(parsed.dir_badge(""), Badge::Conflict);
    }

    #[test]
    fn too_many_folders_make_subfolders_unknown_but_not_the_root() {
        let mut parts = branch_headers(&[]);
        for index in 0..4100 {
            parts.push(format!("? dir{index}/file.txt"));
        }
        let parsed = parse(&parts).expect("parses");
        assert_eq!(parsed.dir_badge("dir0"), Badge::Unknown(Reason::TooLarge));
        assert_eq!(parsed.dir_badge(""), Badge::Modified);
        assert_eq!(parsed.untracked, 4100);
    }

    #[test]
    fn paths_with_spaces_keep_their_folders() {
        let mut parts = branch_headers(&[]);
        parts.push("1 .M N... 100644 100644 100644 aaaa aaaa my dir/sub dir/a b.txt".to_owned());
        let parsed = parse(&parts).expect("parses");
        assert_eq!(parsed.dir_badge("my dir/sub dir"), Badge::Modified);
    }
}
