use encoding_rs::{Encoding, WINDOWS_1250};

use super::{DetectedBy, TextEncoding};

mod binary;
mod decode;
mod detect;
mod encode;
mod props;

const FALLBACK: &Encoding = WINDOWS_1250;

const FRENCH: &str =
    "Le café était déjà prêt à côté de l'hôpital, où la clé du théâtre restait oubliée.";

fn bytes_in(encoding: &'static Encoding, text: &str) -> Vec<u8> {
    let (bytes, _, unmappable) = encoding.encode(text);
    assert!(!unmappable, "{text} fits {}", encoding.name());
    bytes.into_owned()
}

fn with_bom(bom: &[u8], body: &[u8]) -> Vec<u8> {
    [bom, body].concat()
}

fn utf16(text: &str, little: bool) -> Vec<u8> {
    text.encode_utf16()
        .flat_map(|unit| {
            if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            }
        })
        .collect()
}

fn found(encoding: &'static Encoding, bom: bool, by: DetectedBy) -> TextEncoding {
    TextEncoding { encoding, bom, by }
}
