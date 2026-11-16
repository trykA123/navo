use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};
use proptest::prelude::*;

use super::{FALLBACK, found};
use crate::encoding::{DetectedBy, decode, decode_as, encode};

fn unicode() -> impl Strategy<Value = &'static Encoding> {
    prop_oneof![Just(UTF_8), Just(UTF_16LE), Just(UTF_16BE)]
}

proptest! {
    #[test]
    fn text_with_a_bom_survives_encode_then_decode(text in any::<String>(), encoding in unicode()) {
        prop_assume!(!text.starts_with('\0'));
        let bytes = encode(&text, &found(encoding, true, DetectedBy::Bom)).unwrap();
        let decoded = decode(&bytes, FALLBACK);
        prop_assert_eq!(decoded.encoding, found(encoding, true, DetectedBy::Bom));
        prop_assert_eq!(decoded.text, text);
        prop_assert!(decoded.lossless);
    }

    #[test]
    fn text_without_a_bom_survives_encode_then_decode_as(text in any::<String>(), encoding in unicode()) {
        prop_assume!(!text.starts_with('\u{FEFF}'));
        let bytes = encode(&text, &found(encoding, false, DetectedBy::Chosen)).unwrap();
        let decoded = decode_as(&bytes, encoding);
        prop_assert_eq!(decoded.encoding, found(encoding, false, DetectedBy::Chosen));
        prop_assert_eq!(decoded.text, text);
        prop_assert!(decoded.lossless);
    }
}
