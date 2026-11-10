//! Exercise fail-closed manifests, exceptions, retirements, and noise verdicts.

use super::*;
use crate::speed::config::{Accepted, BenchEntry};

const ID: &str = "navo-fs/listing::listing::sort.name";

/// Build a single-ID manifest with a chosen guard state.
fn config(guarded: bool) -> Config {
    Config {
        benches: vec![BenchEntry {
            id: ID.into(),
            guarded,
        }],
        recovery_base: None,
        recovery_reason: None,
        accepted: Vec::new(),
    }
}

/// Build one instruction reading in Ir units.
fn reading(new_ir: u64, old_ir: Option<u64>) -> Measured {
    Measured {
        id: ID.into(),
        new_ir,
        old_ir,
    }
}

/// Run a guard verdict against the fixture base, resolving its tag to its SHA.
fn guard(
    measured: &[Measured],
    head: &Config,
    base: Option<&Config>,
    retired: &[Retired],
) -> Report {
    verdict(
        measured,
        head,
        base,
        retired,
        "base-sha",
        &["navo-fs listing -".into()],
        Mode::Guard,
        50,
        5,
        |reference| (reference == "v0.5.0").then(|| "base-sha".into()),
    )
}

/// Assert failure and a specific finding through the public report.
fn fails(report: Report, finding: &str) {
    assert!(report.failed, "expected failure: {:?}", report.lines);
    assert!(
        report.lines.join("\n").contains(finding),
        "{:?}",
        report.lines
    );
}

#[test]
fn empty_head_manifest_fails_sc4_rule_one() {
    let mut head = config(true);
    head.benches.clear();
    fails(guard(&[], &head, None, &[]), "empty");
}

#[test]
fn duplicate_manifest_ids_fail_sc4_rule_two() {
    let mut head = config(true);
    head.benches.push(BenchEntry {
        id: ID.into(),
        guarded: true,
    });
    fails(
        guard(&[reading(1000, Some(1000))], &head, None, &[]),
        "duplicate manifest",
    );
}

#[test]
fn duplicate_measured_ids_fail_sc4_rule_two() {
    fails(
        guard(
            &[reading(1000, Some(1000)), reading(1000, Some(1000))],
            &config(true),
            None,
            &[],
        ),
        "duplicate measured",
    );
}

#[test]
fn missing_head_summary_fails_sc4_rule_three() {
    fails(guard(&[], &config(true), None, &[]), "no head summary");
}

#[test]
fn unlisted_summary_fails_sc4_rule_four() {
    let mut item = reading(1000, Some(1000));
    item.id = "navo-fs/listing::other".into();
    fails(guard(&[item], &config(true), None, &[]), "unlisted");
}

#[test]
fn target_without_manifest_id_fails_sc4_rule_five() {
    let report = verdict(
        &[reading(1000, Some(1000))],
        &config(true),
        None,
        &[],
        "base-sha",
        &["navo-fs listing -".into(), "navo-index search bench".into()],
        Mode::Guard,
        50,
        5,
        |_| None,
    );
    fails(report, "target navo-index/search has no manifest ID");
}

#[test]
fn manifest_without_target_fails_sc4_rule_five() {
    let report = verdict(
        &[reading(1000, Some(1000))],
        &config(true),
        None,
        &[],
        "base-sha",
        &["navo-index search bench".into()],
        Mode::Guard,
        50,
        5,
        |_| None,
    );
    fails(report, "names no target");
}

#[test]
fn guarded_without_base_ir_fails_even_without_base_manifest() {
    fails(
        guard(&[reading(1000, None)], &config(true), None, &[]),
        "no base Ir",
    );
}

#[test]
fn established_removed_id_fails_sc4_rule_eight() {
    let mut head = config(false);
    head.benches[0].id = "navo-fs/listing::other".into();
    let mut item = reading(1000, None);
    item.id = head.benches[0].id.clone();
    fails(
        guard(&[item], &head, Some(&config(true)), &[]),
        "established",
    );
}

#[test]
fn matching_retirement_allows_removal() {
    let mut head = config(false);
    head.benches[0].id = "navo-fs/listing::other".into();
    let mut item = reading(1000, None);
    item.id = head.benches[0].id.clone();
    let retired = [Retired {
        id: ID.into(),
        base: "v0.5.0".into(),
        reason: "replaced".into(),
    }];
    let report = guard(&[item], &head, Some(&config(true)), &retired);
    assert!(!report.failed, "{:?}", report.lines);
    assert!(report.lines.join("\n").contains("replaced"));
}

#[test]
fn established_downgrade_fails() {
    fails(
        guard(
            &[reading(1000, Some(1000))],
            &config(false),
            Some(&config(true)),
            &[],
        ),
        "established",
    );
}

#[test]
fn retirement_for_another_base_still_fails() {
    let retired = [Retired {
        id: ID.into(),
        base: "another".into(),
        reason: "removed".into(),
    }];
    fails(
        guard(
            &[reading(1000, Some(1000))],
            &config(false),
            Some(&config(true)),
            &retired,
        ),
        "established",
    );
}

#[test]
fn regression_at_limit_plus_point_one_fails() {
    fails(
        guard(&[reading(1051, Some(1000))], &config(true), None, &[]),
        "regressed",
    );
}

#[test]
fn regression_at_and_below_limit_passes() {
    for new_ir in [1049, 1050] {
        assert!(!guard(&[reading(new_ir, Some(1000))], &config(true), None, &[]).failed);
    }
}

/// Build an accepted slowdown with an inclusive maximum percentage.
fn accepted(base: &str) -> Config {
    let mut head = config(true);
    head.accepted.push(Accepted {
        bench: ID.into(),
        base: base.into(),
        max_percent: 6.0,
        reason: "extra validation".into(),
    });
    head
}

#[test]
fn accepted_matching_base_passes_through_maximum() {
    let report = guard(&[reading(1060, Some(1000))], &accepted("v0.5.0"), None, &[]);
    assert!(!report.failed, "{:?}", report.lines);
    assert!(report.lines.join("\n").contains("extra validation"));
}

#[test]
fn accepted_another_base_is_stale_and_regressed() {
    let report = guard(
        &[reading(1051, Some(1000))],
        &accepted("another"),
        None,
        &[],
    );
    assert!(report.lines.join("\n").contains("stale"));
    fails(report, "regressed");
}

#[test]
fn accepted_above_maximum_is_regressed() {
    fails(
        guard(&[reading(1061, Some(1000))], &accepted("v0.5.0"), None, &[]),
        "regressed",
    );
}

#[test]
fn bootstrap_without_base_passes_and_notes_absent_manifest() {
    let report = guard(&[reading(1000, None)], &config(false), None, &[]);
    assert!(!report.failed, "{:?}", report.lines);
    assert!(report.lines[0].contains("no base manifest"));
    assert!(report.lines.join("\n").contains("bootstrap"));
}

#[test]
fn bootstrap_with_base_is_compared_and_flagged() {
    let report = guard(&[reading(1000, Some(1000))], &config(false), None, &[]);
    assert!(!report.failed, "{:?}", report.lines);
    assert!(report.lines.join("\n").contains("set guarded = true"));
    fails(
        guard(&[reading(1051, Some(1000))], &config(false), None, &[]),
        "regressed",
    );
}

/// Run noise mode with one unlisted ID and no established IDs.
fn noise(new_ir: u64, old_ir: Option<u64>) -> Report {
    let mut head = config(true);
    head.benches.clear();
    verdict(
        &[reading(new_ir, old_ir)],
        &head,
        None,
        &[],
        "",
        &[],
        Mode::Noise,
        50,
        5,
        |_| None,
    )
}

#[test]
fn noise_passes_with_unlisted_ids_at_inclusive_boundary() {
    for new_ir in [995, 1000, 1005] {
        let report = noise(new_ir, Some(1000));
        assert!(!report.failed, "{:?}", report.lines);
        assert!(report.lines.join("\n").contains("unlisted"));
    }
}

#[test]
fn noise_fails_in_both_directions_and_on_missing_values() {
    for new_ir in [994, 1006] {
        fails(noise(new_ir, Some(1000)), "noise");
    }
    fails(noise(1000, None), "base Ir");
}

#[test]
fn exact_large_regression_above_limit_fails() {
    fails(
        guard(
            &[reading(9457559217478093, Some(9007199254741040))],
            &config(true),
            None,
            &[],
        ),
        "regressed",
    );
}

#[test]
fn exact_large_noise_at_limit_passes() {
    let report = noise(9052235251015107, Some(9007199254741400));
    assert!(!report.failed, "{:?}", report.lines);
}

#[test]
fn unmeasured_accepted_entry_is_stale_and_fails() {
    let mut head = accepted("v0.5.0");
    head.accepted[0].bench = "navo-fs/listing::absent".into();
    fails(
        guard(&[reading(1000, Some(1000))], &head, None, &[]),
        "stale accepted navo-fs/listing::absent",
    );
}
