use super::{RUSTY_EXE_LIMIT, TooLarge, check_size};

#[test]
fn limit_is_160_mebibytes() {
    assert_eq!(RUSTY_EXE_LIMIT, 167_772_160);
}

#[test]
fn size_equal_to_limit_is_ok() {
    assert_eq!(check_size(1000, 1000), Ok(1000));
}

#[test]
fn size_one_over_limit_is_too_large() {
    assert_eq!(
        check_size(1001, 1000),
        Err(TooLarge {
            bytes: 1001,
            limit: 1000
        })
    );
}
