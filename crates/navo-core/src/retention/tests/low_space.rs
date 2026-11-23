//! Low free space cleanup (AD14): it never removes an item kept in the last 24 hours, while
//! quota and expiry keep their old rules.

use super::*;

/// Choices for two one-byte entries made `ages_ms` before `now`, the first with fewer failed attempts.
fn low_space_choices(ages_ms: [i64; 2], max_volume_bytes: u64) -> Vec<(i64, PurgeReason)> {
    let now = 2 * DAY_MS;
    let inventory: Vec<_> = ages_ms
        .iter()
        .zip(1..)
        .map(|(age, id)| {
            let mut kept = entry(id, 1, now - age);
            kept.record.expires_at = kept.record.created_at.max(now) + DAY_MS;
            kept
        })
        .zip([0, 1])
        .map(|(entry, attempts)| {
            RetentionSummary::try_from(&entry)
                .unwrap()
                .with_attempts(attempts)
        })
        .collect();
    let plan = select_purge_inventory(
        &inventory,
        RetentionPolicy::new(7, max_volume_bytes, 10).unwrap(),
        PurgeRequest {
            space: VolumeSpace {
                volume: 7,
                free_bytes: 0,
            },
            now,
        },
    )
    .unwrap();
    assert!(plan.needs_space_check);
    plan.choices
        .iter()
        .map(|choice| (choice.id.0, choice.reason))
        .collect()
}

#[test]
fn low_space_never_removes_an_item_kept_in_the_last_24_hours() {
    assert_eq!(low_space_choices([0, DAY_MS - 1], 100), []);
}

#[test]
fn low_space_passes_over_a_recent_item_to_an_older_one() {
    assert_eq!(
        low_space_choices([DAY_MS - 1, DAY_MS], 100),
        [(2, PurgeReason::LowSpace)]
    );
}

#[test]
fn low_space_removes_an_item_kept_exactly_24_hours_ago() {
    assert_eq!(
        low_space_choices([DAY_MS, DAY_MS], 100),
        [(1, PurgeReason::LowSpace)]
    );
}

#[test]
fn the_volume_cap_still_removes_recent_items() {
    assert_eq!(low_space_choices([0, 0], 1), [(1, PurgeReason::Quota)]);
}

#[test]
fn low_space_spares_an_item_whose_time_is_ahead_of_the_clock() {
    assert_eq!(low_space_choices([-2 * DAY_MS, -DAY_MS], 100), []);
}
