mod support;

use proptest::prelude::*;
use rusty_automotive::Format;
use support::{intel_hex, parse, read_all, s_records};

fn blocks() -> impl Strategy<Value = Vec<(u32, Vec<u8>)>> {
    prop::collection::vec(
        (1u32..0x3_0000, prop::collection::vec(any::<u8>(), 1..300)),
        1..6,
    )
    .prop_map(|parts| {
        let mut at = 0u32;
        parts
            .into_iter()
            .map(|(gap, data)| {
                let start = at + gap;
                at = start + data.len() as u32;
                (start, data)
            })
            .collect()
    })
}

fn check(format: Format, text: &str, blocks: &[(u32, Vec<u8>)]) -> Result<(), TestCaseError> {
    let image = parse(format, text);
    let expected: Vec<_> = blocks
        .iter()
        .map(|(start, data)| u64::from(*start)..u64::from(*start) + data.len() as u64)
        .collect();
    prop_assert_eq!(&image.ranges, &expected);
    prop_assert!(image.problems.is_empty(), "{:?}", image.problems);
    prop_assert!(image.has_end);
    for (range, (_, data)) in expected.iter().zip(blocks) {
        let bytes = read_all(text, &image, range.clone());
        let wanted: Vec<Option<u8>> = data.iter().copied().map(Some).collect();
        prop_assert_eq!(bytes, wanted);
    }
    Ok(())
}

proptest! {
    #[test]
    fn intel_hex_images_survive_a_round_trip(blocks in blocks()) {
        check(Format::IntelHex, &intel_hex(&blocks, true), &blocks)?;
    }

    #[test]
    fn s_record_images_survive_a_round_trip(blocks in blocks()) {
        check(Format::SRecord, &s_records(&blocks, 0), &blocks)?;
    }

    #[test]
    fn every_gap_lies_between_two_ranges(blocks in blocks()) {
        let image = parse(Format::IntelHex, &intel_hex(&blocks, true));
        let gaps = image.gaps();
        prop_assert_eq!(gaps.len() + 1, image.ranges.len());
        for (gap, pair) in gaps.iter().zip(image.ranges.windows(2)) {
            prop_assert_eq!(gap.clone(), pair[0].end..pair[1].start);
        }
    }
}
