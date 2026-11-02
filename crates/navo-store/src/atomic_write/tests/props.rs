use proptest::prelude::*;

use super::*;

fn any_failure() -> impl Strategy<Value = Option<Stage>> {
    prop::option::of(prop::sample::select(ALL_STAGES.to_vec()))
}

fn bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..4096)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn interrupted_write_leaves_old_or_new_content(a in bytes(), b in bytes(), failure in any_failure()) {
        let dir = tmp();
        let target = dir.path().join("data.bin");
        write_atomic(&target, &a)?;
        let _ = match failure {
            Some(stage) => write_atomic_with(&target, &b, fail_at(stage)),
            None => write_atomic(&target, &b),
        };
        let replaced = matches!(failure, None | Some(Stage::Replaced));
        let want = if replaced { &b } else { &a };
        prop_assert_eq!(&fs::read(&target)?, want, "failed at {:?}", failure);
    }
}
