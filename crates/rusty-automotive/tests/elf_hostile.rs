mod elf_build;

use elf_build::{build, firmware, read};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn random_bytes_after_the_magic_never_panic(
        class in 1u8..=2,
        data in 1u8..=2,
        rest in proptest::collection::vec(any::<u8>(), 0..4096),
    ) {
        let mut bytes = vec![0x7F, b'E', b'L', b'F', class, data, 1];
        bytes.extend(rest);
        let _ = read(&bytes);
    }

    #[test]
    fn corrupting_a_real_file_never_panics(
        is_64 in any::<bool>(),
        is_big in any::<bool>(),
        edits in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..32),
    ) {
        let mut bytes = build(&firmware(is_64, is_big));
        for (at, value) in edits {
            let at = at.index(bytes.len());
            bytes[at] = value;
        }
        let _ = read(&bytes);
    }
}
