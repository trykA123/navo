use super::*;
use crate::FileIdentity;

#[test]
fn adopted_payloads_require_matching_names_a_single_node_and_an_active_owner_reference() {
    let mut adopted = entry(1, 7, 100);
    adopted.record.version = 2;
    adopted.record.payload_name = Some("a".to_owned());
    adopted.record.container = crate::Location::new("/sample/.ferrite-held-ab").unwrap();
    adopted.job_id = Some(1);
    adopted.validate().unwrap();
    let mut invalid = adopted.clone();
    invalid.record.payload_name = Some("../a".to_owned());
    assert!(invalid.validate().is_err());
    let mut orphan = adopted.clone();
    orphan.job_id = None;
    assert!(orphan.validate().is_err());
    orphan.state = RetainedState::Purged;
    assert!(orphan.validate().is_ok());
    adopted.record.nodes[0].kind = RetainedKind::Folder;
    adopted.record.nodes[0].size = 0;
    assert!(adopted.validate().is_err());
}

#[test]
fn containers_keep_validating_under_the_legacy_brand_but_not_across_kinds() {
    let current = entry(1, 7, 100);
    current.validate().unwrap();
    assert!(
        current
            .record
            .container
            .as_str()
            .contains(crate::brand::RETAINED_PREFIX)
    );
    let mut legacy = current.clone();
    legacy.record.container = crate::Location::new("/sample/.ferrite-retained-1").unwrap();
    legacy.validate().unwrap();
    let mut held = current;
    held.record.container = crate::Location::new("/sample/.navo-held-1").unwrap();
    assert!(held.validate().is_err());
}

fn entry(id: i64, size: u64, created_at: i64) -> RetentionEntry {
    let identity = FileIdentity {
        kind: crate::FileIdentityKind::Native,
        volume: 7,
        file: [1; 16],
    };
    RetentionEntry {
        id: RetentionId(id),
        job_id: None,
        pinned: false,
        state: RetainedState::Ready,
        target: None,
        record: RetentionRecord {
            version: 1,
            payload_name: None,
            source: crate::Location::new("/sample/a").unwrap(),
            container: crate::Location::new(format!("/sample/{CONTAINER_PREFIX}{id:x}")).unwrap(),
            parent_identity: identity,
            container_identity: Some(identity),
            nodes: vec![RetainedNode {
                path: vec![],
                kind: RetainedKind::File,
                identity,
                size,
                modified_ms: Some(9),
            }],
            created_at,
            expires_at: created_at + DAY_MS,
        },
    }
}

fn choices(entries: &[RetentionEntry], cap: u64, now: i64) -> PurgePlan {
    select_purge(
        entries,
        RetentionPolicy::new(1, cap, 10).unwrap(),
        PurgeRequest {
            space: VolumeSpace {
                volume: 7,
                free_bytes: 100,
            },
            now,
        },
    )
    .unwrap()
}

#[test]
fn retention_accepts_only_one_to_seven_days_and_a_positive_cap() {
    for days in [0, 8, 255] {
        assert!(RetentionPolicy::new(days, 10, 0).is_err());
    }
    assert!(RetentionPolicy::new(1, 0, 0).is_err());
    assert_eq!(
        RetentionPolicy::new(7, 10, 0).unwrap().expires_at(0),
        Ok(7 * DAY_MS)
    );
    assert!(
        RetentionPolicy::new(7, 10, 0)
            .unwrap()
            .expires_at(i64::MAX)
            .is_err()
    );
}

#[test]
fn expired_entries_are_selected_before_older_unexpired_quota_candidates() {
    let mut old = entry(1, 10, 0);
    old.record.expires_at = 7 * DAY_MS;
    let newer = entry(2, 20, DAY_MS);
    let plan = choices(&[old, newer], 5, 2 * DAY_MS);
    assert_eq!(
        plan.choices,
        vec![
            PurgeChoice {
                id: RetentionId(2),
                reason: PurgeReason::Expired
            },
            PurgeChoice {
                id: RetentionId(1),
                reason: PurgeReason::Quota
            }
        ]
    );
}

#[test]
fn quota_orders_by_creation_time_and_then_identifier() {
    let plan = choices(&[entry(3, 10, 1), entry(2, 10, 0), entry(1, 10, 0)], 10, 0);
    assert_eq!(
        plan.choices
            .iter()
            .map(|choice| choice.id.0)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(plan.remaining_bytes, 10);
}

#[test]
fn pinned_and_unfinished_entries_count_towards_quota_but_are_not_purged() {
    let mut pinned = entry(1, 10, 0);
    pinned.pinned = true;
    let mut preparing = entry(2, 10, 0);
    preparing.state = RetainedState::Preparing;
    let plan = choices(&[pinned, preparing, entry(3, 1, 0)], 5, 2 * DAY_MS);
    assert_eq!(plan.choices.len(), 1);
    assert_eq!(plan.choices[0].id.0, 3);
    assert!(plan.quota_unmet);
}

#[test]
fn low_space_selects_one_entry_and_requires_a_new_observation() {
    let plan = select_purge(
        &[entry(1, 1, 0), entry(2, 1, 0)],
        RetentionPolicy::new(1, 100, 10).unwrap(),
        PurgeRequest {
            space: VolumeSpace {
                volume: 7,
                free_bytes: 0,
            },
            now: 0,
        },
    )
    .unwrap();
    assert_eq!(
        plan.choices,
        vec![PurgeChoice {
            id: RetentionId(1),
            reason: PurgeReason::LowSpace
        }]
    );
    assert!(plan.needs_space_check);
}

#[test]
fn other_volumes_and_terminal_records_do_not_consume_this_volumes_quota() {
    let mut other = entry(2, 99, 0);
    other.record.parent_identity.volume = 8;
    other.record.container_identity.as_mut().unwrap().volume = 8;
    other.record.nodes[0].identity.volume = 8;
    let mut purged = entry(3, 99, 0);
    purged.state = RetainedState::Purged;
    assert!(
        choices(&[entry(1, 1, 0), other, purged], 5, 0)
            .choices
            .is_empty()
    );
}

#[test]
fn node_paths_cannot_escape_the_recorded_tree_or_repeat() {
    let mut record = entry(1, 0, 0).record;
    record.nodes[0].kind = RetainedKind::Folder;
    let mut child = record.nodes[0].clone();
    child.kind = RetainedKind::File;
    child.path = vec!["..".into(), "outside".into()];
    record.nodes.push(child.clone());
    assert!(record.validate().is_err());
    child.path = vec!["valid".into()];
    record.nodes[1] = child.clone();
    assert!(record.validate().is_ok());
    record.nodes.push(child);
    assert!(record.validate().is_err());
}

#[test]
fn a_container_must_be_a_reserved_sibling_on_the_same_volume() {
    let mut record = entry(1, 0, 0).record;
    record.container = crate::Location::new("/other/.ferrite-retained-a").unwrap();
    assert!(record.validate().is_err());
    record.container = crate::Location::new("/sample/arbitrary").unwrap();
    assert!(record.validate().is_err());
}

#[test]
fn raw_provider_paths_round_trip_through_the_persistent_record() {
    let mut record = entry(1, 0, 0).record;
    record.source =
        crate::Location::local(crate::ProviderPath::from_bytes(b"/sample/invalid-\xff")).unwrap();
    let bytes = serde_json::to_vec(&record).unwrap();
    let decoded: RetentionRecord = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        decoded.source.path().to_bytes().unwrap(),
        b"/sample/invalid-\xff"
    );
    assert!(decoded.validate().is_ok());
}

#[test]
fn duplicate_entry_identifiers_are_rejected_by_policy() {
    let entry = entry(1, 10, 0);
    let result = select_purge(
        &[entry.clone(), entry],
        RetentionPolicy::new(1, 5, 0).unwrap(),
        PurgeRequest {
            space: VolumeSpace {
                volume: 7,
                free_bytes: 100,
            },
            now: 0,
        },
    );
    assert!(result.is_err());
}

#[test]
fn persisted_source_paths_cannot_contain_dot_segments() {
    let mut record = entry(1, 10, 0).record;
    record.source = crate::Location::new("/sample/../a").unwrap();
    record.container = crate::Location::new("/sample/../.ferrite-retained-a").unwrap();
    assert!(record.validate().is_err());
}

#[test]
fn compact_inventory_keeps_policy_ordering_and_quota_results() {
    let entries = vec![entry(1, 7, 100), entry(2, 11, 200)];
    let inventory = entries
        .iter()
        .map(RetentionSummary::try_from)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for now in [0, 4 * DAY_MS] {
        let policy = RetentionPolicy::new(3, 8, 10).unwrap();
        let request = PurgeRequest {
            now,
            space: VolumeSpace {
                volume: 7,
                free_bytes: 0,
            },
        };
        assert_eq!(
            select_purge(&entries, policy, request),
            select_purge_inventory(&inventory, policy, request)
        );
    }
}
