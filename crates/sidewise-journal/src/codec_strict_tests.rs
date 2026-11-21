//! Tests of the codec's strict side: raw bytes, the trusted prefix length, unknown fields
//! and the job id shape. Every bad line here carries a valid checksum.

use crate::codec_tests::{every_variant, hex_of, log_of};
use crate::record::{Record, decode, encode};

/// A log line with a correct checksum around any json.
fn line(json: &str) -> String {
    format!("{json}\t{}\n", hex_of(json))
}

/// Asserts that `bad` between two good lines is damage, with only the first line kept.
fn assert_damage(bad: &[u8]) {
    let good = encode(&Record::Commit);
    let mut bytes = good.clone().into_bytes();
    bytes.extend_from_slice(bad);
    bytes.extend_from_slice(good.as_bytes());
    let log = decode(&bytes);
    let shown = String::from_utf8_lossy(bad);
    assert!(log.damaged, "line {shown:?} must be damage");
    assert_eq!(log.records, vec![Record::Commit], "line {shown:?}");
    assert_eq!(
        log.good_len,
        good.len(),
        "the trusted prefix ends before {shown:?}"
    );
}

#[test]
fn codec_strict_non_utf8_line_is_damage() {
    let mut bad = b"{\"t\":\"Aborted\",\"reason\":\"".to_vec();
    bad.extend_from_slice(&[0xff, 0xfe]);
    bad.extend_from_slice(b"\"}");
    // The checksum holds over the raw bytes, so only the UTF-8 rule can reject the line.
    let hash = blake3::hash(&bad);
    let hex: String = hash.as_bytes()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    bad.extend_from_slice(format!("\t{hex}\n").as_bytes());
    assert_damage(&bad);
}

#[test]
fn codec_strict_non_utf8_tail_is_torn_not_damage() {
    let records = every_variant();
    let good = log_of(&records[..2]);
    let mut bytes = good.clone().into_bytes();
    bytes.extend_from_slice(&[b'{', 0xc3]);
    let log = decode(&bytes);
    assert!(log.torn_tail, "an unterminated bad tail is a torn tail");
    assert!(
        !log.damaged,
        "bad bytes in a torn tail never fail the whole file"
    );
    assert_eq!(log.records, records[..2]);
    assert_eq!(
        log.good_len,
        good.len(),
        "the writer cuts the file back to here"
    );
}

#[test]
fn codec_strict_good_len_is_zero_without_a_good_line() {
    assert_eq!(decode(b"").good_len, 0);
    assert_eq!(decode(b"{\"t\":\"Com").good_len, 0);
    assert_eq!(decode(b"\n").good_len, 0);
}

#[test]
fn codec_strict_unknown_field_is_damage() {
    let stamp = format!(
        "{{\"size\":1,\"modified_ms\":2,\"hash\":\"{}\"",
        "ab".repeat(32)
    );
    let bad_json = [
        r#"{"t":"Done","item":0,"extra":1}"#.to_owned(),
        r#"{"t":"Commit","extra":1}"#.to_owned(),
        r#"{"t":"Aborted","reason":"x","t2":"Done"}"#.to_owned(),
        format!("{{\"t\":\"Staged\",\"item\":0,\"new\":{stamp},\"extra\":1}}}}"),
    ];
    for json in bad_json {
        assert_damage(line(&json).as_bytes());
    }
}

#[test]
fn codec_strict_bad_job_id_is_damage() {
    let ids = [
        "../etc",
        "123-1-2",
        "0000000000001-1-",
        "0000000000001--2",
        "x",
    ];
    for id in ids {
        let begin = format!(
            r#"{{"t":"Begin","job":"{id}","kind":"Copy","label":"l","created_ms":1,"reverts":null,"owner":"o"}}"#
        );
        assert_damage(line(&begin).as_bytes());
        let reverts = format!(
            r#"{{"t":"Begin","job":"0000000000001-1-2","kind":"Undo","label":"l","created_ms":1,"reverts":"{id}","owner":"o"}}"#
        );
        assert_damage(line(&reverts).as_bytes());
    }
}

#[test]
fn codec_strict_good_job_ids_decode() {
    let begin = r#"{"t":"Begin","job":"0000000000001-1-2","kind":"Undo","label":"l","created_ms":1,"reverts":"18446744073709551615-4294967295-0","owner":"o"}"#;
    let log = decode(line(begin).as_bytes());
    assert!(!log.damaged, "a 13- or 20-digit time is a good id");
    assert_eq!(log.records.len(), 1);
}
