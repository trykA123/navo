use crate::Location;
use crate::rename::check::{Existing, Fold, Problem, Row, RowState, check_rows};
use crate::rename::{Cleanup, Compiled, RenameRules, Subject, split_name};

fn run(cleanup: Cleanup, name: &str) -> String {
    let rules = RenameRules {
        cleanup,
        ..RenameRules::default()
    };
    let compiled = Compiled::new(&rules).unwrap_or_else(|e| panic!("rules should compile: {e}"));
    let (stem, ext) = split_name(name);
    let subject = Subject {
        stem,
        ext,
        parent: "",
        grand: "",
        time: None,
        fields: None,
    };
    compiled.new_name(&subject, 0)
}

fn only(keep: &str) -> Cleanup {
    Cleanup {
        trim: keep == "trim",
        collapse: keep == "collapse",
        remove_accents: keep == "accents",
        spaces_to_underscore: keep == "underscore",
    }
}

#[test]
fn default_cleanup_changes_nothing() {
    assert_eq!(
        run(Cleanup::default(), "  a  b--c é .TXT"),
        "  a  b--c é .TXT"
    );
}

#[test]
fn trim_cuts_both_ends_of_stem_and_ext() {
    let c = only("trim");
    assert_eq!(run(c, "  a b  .txt"), "a b.txt");
    assert_eq!(run(c, "a . txt "), "a.txt");
    assert_eq!(run(c, "a  b.txt"), "a  b.txt");
}

#[test]
fn collapse_merges_runs_of_the_same_separator() {
    let c = only("collapse");
    assert_eq!(run(c, "a   b__c--d..e.txt"), "a b_c-d.e.txt");
    assert_eq!(run(c, "a - _ b.txt"), "a - _ b.txt");
    assert_eq!(run(c, "aa bb.txt"), "aa bb.txt");
}

#[test]
fn accents_drop_marks_and_keep_undecomposable_letters() {
    let c = only("accents");
    assert_eq!(run(c, "Ștefan Țepeș ăîâ.txt"), "Stefan Tepes aia.txt");
    assert_eq!(
        run(c, "café crème à la française.txt"),
        "cafe creme a la francaise.txt"
    );
    assert_eq!(run(c, "año mañana.txt"), "ano manana.txt");
    assert_eq!(run(c, "Größe über Straße.txt"), "Große uber Straße.txt");
    assert_eq!(run(c, "ø đ.txt"), "ø đ.txt");
    assert_eq!(run(c, "a.ŢXT"), "a.TXT");
}

#[test]
fn spaces_become_underscores() {
    let c = only("underscore");
    assert_eq!(run(c, "a b  c.txt"), "a_b__c.txt");
    assert_eq!(run(c, " a .txt"), "_a_.txt");
}

#[test]
fn toggles_combine_in_a_fixed_order() {
    let all = Cleanup {
        trim: true,
        collapse: true,
        remove_accents: true,
        spaces_to_underscore: true,
    };
    assert_eq!(
        run(all, "  Ștefan   cel  Mare  .txt"),
        "Stefan_cel_Mare.txt"
    );
    assert_eq!(run(all, "Crème -- brûlée.PDF"), "Creme_-_brulee.PDF");
    let trim_collapse = Cleanup {
        trim: true,
        collapse: true,
        ..Cleanup::default()
    };
    assert_eq!(run(trim_collapse, "  a   b  "), "a b");
}

#[test]
fn a_name_that_empties_becomes_a_problem() {
    let new = run(only("trim"), "   ");
    assert_eq!(new, "");
    let parent = Location::new("/dir").expect("location should parse");
    let rows = [Row {
        parent,
        old: "   ".to_owned(),
        new,
        excluded: false,
    }];
    assert_eq!(
        check_rows(&rows, &Existing::new(Vec::new()), Fold::Exact),
        [RowState::Problem(Problem::Empty)]
    );
}

#[test]
fn old_json_without_cleanup_loads() {
    let rules: RenameRules =
        serde_json::from_str(r#"{"name_case":"title"}"#).expect("old rules should parse");
    assert_eq!(rules.cleanup, Cleanup::default());
    let partial: RenameRules =
        serde_json::from_str(r#"{"cleanup":{"trim":true}}"#).expect("partial cleanup should parse");
    assert!(partial.cleanup.trim && !partial.cleanup.collapse);
}
