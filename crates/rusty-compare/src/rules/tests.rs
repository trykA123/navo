use rusty_core::ProviderPath;

use super::*;
use crate::model::RelPath;

fn rel(text: &str) -> RelPath {
    RelPath::new(ProviderPath::from_text(text))
}

fn rules(
    gitignores: &[(&str, &str)],
    user: &[&str],
    show_ignored: bool,
    case: CaseRule,
) -> IgnoreRules {
    let sources = gitignores
        .iter()
        .map(|(folder, contents)| IgnoreSource::Gitignore {
            folder: rel(folder),
            contents: (*contents).to_owned(),
        });
    let user: Vec<String> = user.iter().map(|line| (*line).to_owned()).collect();
    IgnoreRules::new(sources, &user, show_ignored, case)
        .unwrap()
        .0
}

#[test]
fn a_pattern_without_a_slash_ignores_matching_files_at_any_depth() {
    let rules = rules(&[("", "*.log\n")], &[], false, CaseRule::Sensitive);

    assert!(rules.is_ignored(&rel("a.log"), false));
    assert!(rules.is_ignored(&rel("sub/a.log"), false));
    assert!(!rules.is_ignored(&rel("a.txt"), false));
}

#[test]
fn a_negated_pattern_re_includes_the_file() {
    let rules = rules(
        &[("", "*.log\n!keep.log\n")],
        &[],
        false,
        CaseRule::Sensitive,
    );

    assert!(!rules.is_ignored(&rel("keep.log"), false));
    assert!(!rules.is_ignored(&rel("sub/keep.log"), false));
    assert!(rules.is_ignored(&rel("drop.log"), false));
}

#[test]
fn a_folder_only_pattern_ignores_the_folder_and_its_contents() {
    let rules = rules(&[("", "target/\n")], &[], false, CaseRule::Sensitive);

    assert!(rules.is_ignored(&rel("target"), true));
    assert!(!rules.is_ignored(&rel("target"), false));
    assert!(rules.is_ignored(&rel("target/out.bin"), false));
    assert!(!rules.enters(&rel("target")));
    assert!(rules.enters(&rel("src")));
}

#[test]
fn a_nested_gitignore_file_applies_inside_its_folder() {
    let rules = rules(
        &[("", "*.tmp\n"), ("sub", "!keep.tmp\n")],
        &[],
        false,
        CaseRule::Sensitive,
    );

    assert!(rules.is_ignored(&rel("keep.tmp"), false));
    assert!(rules.is_ignored(&rel("sub/other.tmp"), false));
    assert!(!rules.is_ignored(&rel("sub/keep.tmp"), false));
    assert!(!rules.is_ignored(&rel("sub/deep/keep.tmp"), false));
}

#[test]
fn an_anchored_pattern_matches_only_its_folder_root() {
    let rules = rules(&[("sub", "/local.txt\n")], &[], false, CaseRule::Sensitive);

    assert!(rules.is_ignored(&rel("sub/local.txt"), false));
    assert!(!rules.is_ignored(&rel("sub/deep/local.txt"), false));
    assert!(!rules.is_ignored(&rel("local.txt"), false));
}

#[test]
fn a_repeated_folder_name_is_not_stripped_twice() {
    let rules = rules(&[("sub", "/x\n")], &[], false, CaseRule::Sensitive);

    assert!(rules.is_ignored(&rel("sub/x"), false));
    assert!(!rules.is_ignored(&rel("sub/sub/x"), false));
    assert!(!rules.is_ignored(&rel("sub/deep/x"), false));
}

#[test]
fn recursive_patterns_match_at_any_depth() {
    let rules = rules(
        &[("", "**/build\n"), ("", "a/**/b\n")],
        &[],
        false,
        CaseRule::Sensitive,
    );

    assert!(rules.is_ignored(&rel("build"), true));
    assert!(rules.is_ignored(&rel("x/build"), true));
    assert!(rules.is_ignored(&rel("a/b"), false));
    assert!(rules.is_ignored(&rel("a/x/b"), false));
    assert!(!rules.is_ignored(&rel("a/x/c"), false));
}

#[test]
fn a_sensitive_rule_keeps_case() {
    let rules = rules(&[("Sub", "*.LOG\n")], &[], false, CaseRule::Sensitive);

    assert!(rules.is_ignored(&rel("Sub/a.LOG"), false));
    assert!(!rules.is_ignored(&rel("sub/a.LOG"), false));
    assert!(!rules.is_ignored(&rel("Sub/a.log"), false));
    assert!(!rules.is_ignored(&rel(".GIT"), true));
    assert!(rules.is_ignored(&rel(".git"), true));
}

#[test]
fn an_insensitive_rule_folds_case() {
    let rules = rules(&[("Sub", "*.LOG\n")], &[], false, CaseRule::Insensitive);

    assert!(rules.is_ignored(&rel("sub/a.log"), false));
    assert!(rules.is_ignored(&rel("SUB/A.LOG"), false));
    assert!(rules.is_ignored(&rel(".GIT"), true));
    assert!(!rules.enters(&rel(".GIT")));
}

#[test]
fn a_literal_backslash_is_not_a_separator() {
    let rules = rules(&[("", "/a/b\n")], &[], false, CaseRule::Sensitive);

    assert!(rules.is_ignored(&rel("a/b"), false));
    assert!(!rules.is_ignored(&rel(r"a\b"), false));
}

#[test]
fn a_backslash_name_matches_as_one_segment() {
    let star = rules(&[("", "a*\n")], &[], false, CaseRule::Sensitive);
    assert!(star.is_ignored(&rel(r"a\b"), false));

    let anchored = rules(&[("", "/a\n")], &[], false, CaseRule::Sensitive);
    assert!(!anchored.is_ignored(&rel(r"a\b"), false));

    let user = rules(&[], &["a*"], false, CaseRule::Sensitive);
    assert!(user.is_ignored(&rel(r"a\b"), false));
}

#[test]
fn many_nested_sources_keep_their_folder_scope() {
    let mut sources = Vec::new();
    let mut folder = String::new();
    for depth in 0..64 {
        if depth > 0 {
            folder.push('/');
        }
        folder.push_str(&format!("d{depth}"));
        sources.push(IgnoreSource::Gitignore {
            folder: rel(&folder),
            contents: format!("*.tmp{depth}\n"),
        });
    }
    let (rules, problems) = IgnoreRules::new(sources, &[], false, CaseRule::Sensitive).unwrap();

    assert!(problems.is_empty());
    assert!(rules.is_ignored(&rel(&format!("{folder}/x.tmp63")), false));
    assert!(rules.is_ignored(&rel(&format!("{folder}/x.tmp0")), false));
    assert!(!rules.is_ignored(&rel("e/x.tmp63"), false));
}

#[test]
fn one_bad_line_keeps_the_good_lines_and_is_reported() {
    let source = IgnoreSource::Gitignore {
        folder: rel("sub"),
        contents: "*.log\n[z-a]\n!keep.log\n".to_owned(),
    };
    let (rules, problems) = IgnoreRules::new([source], &[], false, CaseRule::Sensitive).unwrap();

    assert!(rules.is_ignored(&rel("sub/a.log"), false));
    assert!(!rules.is_ignored(&rel("sub/keep.log"), false));
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].source, RuleSource::Gitignore(rel("sub")));
    assert_eq!(problems[0].line, 2);
}

#[test]
fn a_bad_line_in_a_root_gitignore_names_the_gitignore_file() {
    let sources = [
        IgnoreSource::Exclude {
            contents: "*.bak\n".to_owned(),
        },
        IgnoreSource::Gitignore {
            folder: RelPath::root(),
            contents: "*.log\n[z-a]\n".to_owned(),
        },
    ];
    let (rules, problems) = IgnoreRules::new(sources, &[], false, CaseRule::Sensitive).unwrap();

    assert!(rules.is_ignored(&rel("old.bak"), false));
    assert!(rules.is_ignored(&rel("a.log"), false));
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].source, RuleSource::Gitignore(RelPath::root()));
    assert_eq!(problems[0].source.to_string(), ".gitignore");
    assert_eq!(problems[0].line, 2);
}

#[test]
fn the_repository_exclude_file_is_overridden_by_a_gitignore_whitelist() {
    let sources = [
        IgnoreSource::Exclude {
            contents: "*.bak\n".to_owned(),
        },
        IgnoreSource::Gitignore {
            folder: RelPath::root(),
            contents: "!keep.bak\n".to_owned(),
        },
    ];
    let rules = IgnoreRules::new(sources, &[], false, CaseRule::Sensitive)
        .unwrap()
        .0;

    assert!(rules.is_ignored(&rel("old.bak"), false));
    assert!(rules.is_ignored(&rel("sub/old.bak"), false));
    assert!(!rules.is_ignored(&rel("keep.bak"), false));
}

#[test]
fn the_user_list_applies_after_repository_rules() {
    let rules = rules(
        &[("", "*.log\n")],
        &["!keep.log", "scratch/"],
        false,
        CaseRule::Sensitive,
    );

    assert!(rules.is_ignored(&rel("drop.log"), false));
    assert!(!rules.is_ignored(&rel("keep.log"), false));
    assert!(rules.is_ignored(&rel("scratch"), true));
    assert!(!rules.enters(&rel("scratch")));
}

#[test]
fn dot_git_is_always_skipped() {
    let rules = rules(&[], &["!.git"], true, CaseRule::Sensitive);

    assert!(rules.is_ignored(&rel(".git"), true));
    assert!(rules.is_ignored(&rel(".git/config"), false));
    assert!(rules.is_ignored(&rel("sub/.git"), true));
    assert!(!rules.keeps(&rel(".git"), true));
    assert!(!rules.enters(&rel(".git")));
}

#[test]
fn showing_ignored_keeps_the_row_but_does_not_enter_it() {
    let hidden = rules(&[("", "build/\n")], &[], false, CaseRule::Sensitive);
    assert!(!hidden.keeps(&rel("build"), true));
    assert!(!hidden.enters(&rel("build")));

    let shown = rules(&[("", "build/\n")], &[], true, CaseRule::Sensitive);
    assert!(shown.is_ignored(&rel("build"), true));
    assert!(shown.keeps(&rel("build"), true));
    assert!(shown.keeps(&rel("build/out.bin"), false));
    assert!(!shown.enters(&rel("build")));
    assert!(shown.keeps(&rel("src"), true));
}
