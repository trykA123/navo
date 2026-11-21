//! Tests for the hash cache record format and key digest.

use navo_compare::model::ContentKey;
use navo_core::{FileIdentity, FileIdentityKind, ProviderId, ProviderPath};

use crate::compare::hash_cache::record::*;

const NOW_MS: i64 = 1_700_000_000_000;

fn key() -> ContentKey {
    ContentKey {
        provider: ProviderId::local(),
        full_path: ProviderPath::from_text("/data/a.bin"),
        size: 10,
        modified: Some(NOW_MS),
        file_id: Some(FileIdentity {
            volume: 7,
            file: [3; 16],
            kind: FileIdentityKind::Native,
        }),
    }
}

#[test]
fn a_record_round_trips() {
    let record = encode(&[1; 32], &[2; 32]);
    assert_eq!(decode(&record), Some(([1; 32], [2; 32])));
}

#[test]
fn a_flipped_bit_fails_the_check() {
    for byte in [0, 40, 70] {
        let mut record = encode(&[1; 32], &[2; 32]);
        record[byte] ^= 1;
        assert_eq!(decode(&record), None, "byte {byte}");
    }
}

#[test]
fn replay_skips_a_bad_record_and_flags_a_torn_tail() {
    let mut bytes = MAGIC.to_vec();
    bytes.extend(encode(&[1; 32], &[1; 32]));
    let mut bad = encode(&[2; 32], &[2; 32]);
    bad[33] ^= 1;
    bytes.extend(bad);
    bytes.extend(encode(&[3; 32], &[3; 32]));
    bytes.extend([9; 10]);
    let replay = replay(&bytes);
    assert!(replay.header_ok);
    assert_eq!(replay.bad, 1);
    assert!(replay.torn);
    assert_eq!(replay.rows, vec![([1; 32], [1; 32]), ([3; 32], [3; 32])]);
}

#[test]
fn a_wrong_header_ignores_the_file() {
    let empty = replay(&[]);
    assert!(empty.header_ok && empty.rows.is_empty());
    let short = replay(b"SWHA");
    assert!(!short.header_ok && short.rows.is_empty());
    let mut bytes = b"SWHASH\x00\x02".to_vec();
    bytes.extend(encode(&[1; 32], &[1; 32]));
    let wrong = replay(&bytes);
    assert!(!wrong.header_ok && wrong.rows.is_empty());
}

#[test]
fn keys_without_identity_time_or_local_provider_get_no_digest() {
    assert!(key_digest(&key()).is_some());
    let mut no_id = key();
    no_id.file_id = None;
    assert_eq!(key_digest(&no_id), None);
    let mut no_time = key();
    no_time.modified = None;
    assert_eq!(key_digest(&no_time), None);
    let mut git = key();
    git.provider = ProviderId::new("git").unwrap();
    assert_eq!(key_digest(&git), None);
}

#[test]
fn a_fingerprint_identity_gets_no_digest() {
    let mut fingerprint = key();
    fingerprint.file_id.as_mut().unwrap().kind = FileIdentityKind::Fingerprint { folder: false };
    assert_eq!(key_digest(&fingerprint), None);
}

#[test]
fn a_pre_1990_time_gets_no_digest() {
    let mut old = key();
    old.modified = Some(OLDEST_MODIFIED_MS - 1);
    assert_eq!(key_digest(&old), None);
    old.modified = Some(OLDEST_MODIFIED_MS);
    assert!(key_digest(&old).is_some());
}

#[test]
fn size_time_and_identity_each_change_the_digest() {
    let base = key_digest(&key()).unwrap();
    let mut size = key();
    size.size += 1;
    let mut time = key();
    time.modified = Some(NOW_MS + 1);
    let mut volume = key();
    volume.file_id.as_mut().unwrap().volume += 1;
    let mut file = key();
    file.file_id.as_mut().unwrap().file[0] ^= 1;
    let mut kind = key();
    kind.file_id.as_mut().unwrap().kind = FileIdentityKind::WindowsIndex;
    let mut path = key();
    path.full_path = ProviderPath::from_text("/data/b.bin");
    for changed in [size, time, volume, file, kind, path] {
        assert_ne!(key_digest(&changed), Some(base));
    }
    assert_eq!(key_digest(&key()), Some(base));
}
