//! Unit tests for the build-machine path and session-link guard.

use super::{Allow, Hit, load_personal_needles, personal_hits, scan};

/// Return the 1-based line numbers of `hits`, in order.
fn lines(hits: &[Hit]) -> Vec<usize> {
    hits.iter().map(|hit| hit.line).collect()
}

#[test]
fn flags_a_session_link_in_rust() {
    let hits = scan(
        "crates/x/src/a.rs",
        "// see https://claude.ai/code/session_01",
        &[],
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 1);
}

#[test]
fn ignores_home_paths_in_rust() {
    assert!(scan("crates/x/src/a.rs", "let p = \"/home/admin/x\";", &[]).is_empty());
}

#[test]
fn flags_mnt_and_home_in_markdown() {
    let text = "a\n/mnt/x/y\n/home/bob/z\n<user>: /home/<user>/q";
    assert_eq!(lines(&scan("docs/a.md", text, &[])), vec![2, 3]);
}

#[test]
fn flags_windows_user_folder() {
    let hit = scan("docs/a.md", "C:\\Users\\bob\\AppData", &[]);
    assert_eq!(lines(&hit), vec![1]);
    let placeholder = scan("docs/a.md", "C:\\Users\\<user>\\AppData", &[]);
    assert!(placeholder.is_empty());
}

#[test]
fn allow_line_silences_a_hit() {
    let allow = [Allow {
        path: "docs/a.md".to_string(),
        needle: "/mnt/data".to_string(),
    }];
    assert!(scan("docs/a.md", "/mnt/data/x", &allow).is_empty());
}

#[test]
fn fixtures_are_skipped_for_paths() {
    assert!(scan("crates/x/tests/fixtures/v1.sql", "/home/admin/", &[]).is_empty());
}

#[test]
fn flags_windows_forward_slash_and_any_case() {
    assert_eq!(lines(&scan("docs/a.md", "C:/Users/bob/x", &[])), vec![1]);
    assert_eq!(lines(&scan("docs/a.md", "d:/users/bob/x", &[])), vec![1]);
}

#[test]
fn flags_macos_home() {
    assert_eq!(lines(&scan("docs/a.md", "/Users/bob/notes", &[])), vec![1]);
}

#[test]
fn flags_claude_handoff_dir() {
    assert_eq!(
        lines(&scan("plans/a.md", "see ~/.claude/handoffs/crew/x", &[])),
        vec![1]
    );
}

#[test]
fn flags_home_without_trailing_slash() {
    let text = "cd /home/bob\n/home/bob.";
    assert_eq!(lines(&scan("docs/a.md", text, &[])), vec![1, 2]);
}

#[test]
fn allow_line_needs_the_whole_path_token() {
    let prefix = [Allow {
        path: "docs/a.md".to_string(),
        needle: "/mnt/".to_string(),
    }];
    assert_eq!(lines(&scan("docs/a.md", "/mnt/real/x", &prefix)), vec![1]);
    let exact = [Allow {
        path: "docs/a.md".to_string(),
        needle: "/mnt/real/x".to_string(),
    }];
    assert!(scan("docs/a.md", "/mnt/real/x", &exact).is_empty());
}

#[test]
fn personal_needles_load_from_a_temp_file() {
    let dir = std::env::temp_dir().join(format!("navo-paths-deny-{}", std::process::id()));
    assert!(std::fs::create_dir_all(&dir).is_ok());
    let path = dir.join("navo-paths-deny");
    assert!(std::fs::write(&path, "# comment\n/opt/private/\n\n/navo/secret\n").is_ok());
    let needles = load_personal_needles(&path).unwrap_or_default();
    assert_eq!(needles, ["/opt/private/", "/navo/secret"]);
    assert_eq!(
        personal_hits("let p = \"/opt/private/x\";\n", &needles),
        vec![1]
    );
    assert!(std::fs::remove_dir_all(&dir).is_ok());
}
