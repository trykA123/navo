#![cfg(windows)]

use navo_shell::{SealError, seal, unseal};

const SECRET: &[u8] = b"invented-password-for-tests-7f3a";

#[test]
fn dpapi_round_trip_returns_the_input() {
    let sealed = seal(SECRET).expect("seal");
    let opened = unseal(&sealed).expect("unseal");
    assert_eq!(opened.expose(), SECRET);
}

#[test]
fn dpapi_flipped_byte_fails_to_unseal() {
    let sealed = seal(SECRET).expect("seal");
    for index in [sealed.len() / 2, sealed.len() - 1] {
        let mut damaged = sealed.clone();
        damaged[index] ^= 0x01;
        let result = unseal(&damaged);
        assert!(
            matches!(result, Err(SealError::Os { .. })),
            "a damaged blob must never unseal (byte {index})"
        );
    }
}

#[test]
fn dpapi_sealed_bytes_do_not_contain_the_input() {
    let sealed = seal(SECRET).expect("seal");
    assert!(
        !sealed.windows(SECRET.len()).any(|window| window == SECRET),
        "the sealed blob must not hold plain text"
    );
}

#[test]
fn dpapi_empty_input_is_refused() {
    assert_eq!(seal(b"").err(), Some(SealError::Empty));
    assert_eq!(unseal(b"").err(), Some(SealError::Empty));
}

#[test]
fn dpapi_large_input_round_trips() {
    let large: Vec<u8> = (0..256 * 1024).map(|index| (index % 251) as u8).collect();
    let sealed = seal(&large).expect("seal");
    let opened = unseal(&sealed).expect("unseal");
    assert_eq!(opened.expose(), large.as_slice());
}

#[test]
fn dpapi_garbage_is_refused() {
    let result = unseal(&[0x5a; 64]);
    assert!(matches!(result, Err(SealError::Os { .. })));
}
