use proptest::prelude::*;

use crate::{CancelToken, FakeFs, FolderProfile, Tally, folder_profile, folder_size};

fn name(index: usize, upper: bool, folder: u8) -> String {
    let extension = format!("k{index}");
    let extension = if upper {
        extension.to_uppercase()
    } else {
        extension
    };
    format!("d/f{folder}/n{index}-{upper}.{extension}")
}

proptest! {
    #[test]
    fn totals_equal_the_sum_over_the_kinds(
        files in prop::collection::vec((0_usize..400, any::<bool>(), 0_u8..4, 0_u64..10_000), 0..600)
    ) {
        let fs = FakeFs::new().with_batch_size(7);
        for (index, upper, folder, size) in &files {
            fs.add_file(&name(*index, *upper, *folder), *size);
        }
        fs.add_folder("d");
        let profile = folder_profile(&fs, &fs.location("d"), &CancelToken::new(), &mut |_: &FolderProfile| {});
        let sum = profile.kinds.iter().fold(profile.other, |acc, (_, t)| Tally {
            bytes: acc.bytes + t.bytes,
            files: acc.files + t.files,
        });
        prop_assert_eq!(sum, Tally { bytes: profile.totals.bytes, files: profile.totals.files });
        prop_assert!(profile.kinds.len() <= 64);
        prop_assert!(profile.largest.len() <= 10);
        let size = folder_size(&fs, &fs.location("d"), &CancelToken::new()).unwrap();
        prop_assert_eq!(profile.totals, size);
    }
}
