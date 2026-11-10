//! Exercise SC3 release filtering and strict recovery ancestry without git or I/O.

use super::*;

/// Build a release fixture with its product tag, commit and draft state.
fn release(tag: &str, commit: &str, is_draft: bool) -> Release {
    Release {
        tag: tag.into(),
        commit: commit.into(),
        is_draft,
    }
}

/// Model a linear history and one unrelated commit for ancestry checks.
fn is_ancestor(base: &str, head: &str) -> bool {
    let history = ["old", "release", "newer", "recovery", "head"];
    match (
        history.iter().position(|value| *value == base),
        history.iter().position(|value| *value == head),
    ) {
        (Some(base), Some(head)) => base <= head,
        _ => false,
    }
}

#[test]
fn highest_strict_ancestor_product_tag_is_chosen() -> Result<(), String> {
    let releases = [
        release("v1.9.0", "release", false),
        release("v1.10.0", "newer", false),
        release("v1.8.9", "old", false),
    ];
    let base = choose_base(&releases, "head", None, is_ancestor)?;
    assert_eq!(
        base,
        Base {
            sha: "newer".into(),
            label: "v1.10.0".into()
        }
    );
    Ok(())
}

#[test]
fn newer_bench_prerelease_is_ignored() -> Result<(), String> {
    let releases = [
        release("v1.2.3", "release", false),
        release("bench-2026-11-11", "newer", false),
    ];
    assert_eq!(
        choose_base(&releases, "head", None, is_ancestor)?.sha,
        "release"
    );
    Ok(())
}

#[test]
fn prerelease_product_tag_counts() -> Result<(), Box<dyn std::error::Error>> {
    let info: super::ReleaseInfo =
        serde_json::from_str(r#"{"tagName":"v1.2.3","isDraft":false,"isPrerelease":true}"#)?;
    let releases = [release(&info.tag_name, "release", info.is_draft)];
    assert_eq!(
        choose_base(&releases, "head", None, is_ancestor)?.sha,
        "release"
    );
    Ok(())
}

#[test]
fn draft_is_ignored() -> Result<(), String> {
    let releases = [
        release("v1.2.3", "release", false),
        release("v2.0.0", "newer", true),
    ];
    assert_eq!(
        choose_base(&releases, "head", None, is_ancestor)?.sha,
        "release"
    );
    Ok(())
}

#[test]
fn release_on_head_is_skipped() -> Result<(), String> {
    let releases = [
        release("v1.2.3", "release", false),
        release("v2.0.0", "head", false),
    ];
    assert_eq!(
        choose_base(&releases, "head", None, is_ancestor)?.sha,
        "release"
    );
    Ok(())
}

#[test]
fn non_ancestor_is_skipped() -> Result<(), String> {
    let releases = [
        release("v1.2.3", "release", false),
        release("v2.0.0", "unrelated", false),
    ];
    assert_eq!(
        choose_base(&releases, "head", None, is_ancestor)?.sha,
        "release"
    );
    Ok(())
}

#[test]
fn incomplete_and_suffixed_versions_are_ignored() -> Result<(), String> {
    let releases = [
        release("v1.0.0", "release", false),
        release("v1.2", "newer", false),
        release("v1.2.3-rc1", "newer", false),
    ];
    assert_eq!(
        choose_base(&releases, "head", None, is_ancestor)?.sha,
        "release"
    );
    Ok(())
}

#[test]
fn no_eligible_release_is_an_error() {
    assert!(choose_base(&[], "head", None, is_ancestor).is_err());
    assert!(choose_base(&[], "head", Some("recovery"), is_ancestor).is_err());
}

#[test]
fn valid_recovery_is_accepted() -> Result<(), String> {
    let releases = [release("v1.2.3", "release", false)];
    let base = choose_base(&releases, "head", Some("recovery"), is_ancestor)?;
    assert_eq!(
        base,
        Base {
            sha: "recovery".into(),
            label: "recovery".into()
        }
    );
    Ok(())
}

#[test]
fn recovery_must_be_strictly_between_release_and_head() {
    let releases = [release("v1.2.3", "release", false)];
    for recovery in ["head", "old", "unrelated", "release"] {
        assert!(
            choose_base(&releases, "head", Some(recovery), is_ancestor).is_err(),
            "accepted {recovery}"
        );
    }
}
