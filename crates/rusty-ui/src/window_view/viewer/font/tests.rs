use std::io::Write as _;

use flate2::Compression;
use flate2::write::ZlibEncoder;

use super::face::{FontFace, FontFormat, font_format};

const ARCHIVO: &[u8] = include_bytes!("../../../../fonts/Archivo-Regular.ttf");

fn be16(bytes: &[u8], at: usize) -> usize {
    usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]))
}

fn be32(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
}

fn put32(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&u32::try_from(value).unwrap().to_be_bytes());
}

fn woff_of(sfnt: &[u8]) -> Vec<u8> {
    let count = be16(sfnt, 4);
    let mut directory = Vec::new();
    let mut data = Vec::new();
    let start = 44 + 20 * count;
    for index in 0..count {
        let record = 12 + 16 * index;
        let (offset, length) = (be32(sfnt, record + 8), be32(sfnt, record + 12));
        let table = &sfnt[offset..offset + length];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(table).unwrap();
        let packed = encoder.finish().unwrap();
        let stored = if packed.len() < length {
            packed
        } else {
            table.to_vec()
        };
        directory.extend_from_slice(&sfnt[record..record + 4]);
        put32(&mut directory, start + data.len());
        put32(&mut directory, stored.len());
        put32(&mut directory, length);
        directory.extend_from_slice(&sfnt[record + 4..record + 8]);
        data.extend_from_slice(&stored);
        data.resize(data.len().next_multiple_of(4), 0);
    }
    let mut woff = b"wOFF".to_vec();
    woff.extend_from_slice(&sfnt[..4]);
    put32(&mut woff, start + data.len());
    woff.extend_from_slice(&u16::try_from(count).unwrap().to_be_bytes());
    woff.resize(44, 0);
    woff.extend_from_slice(&directory);
    woff.extend_from_slice(&data);
    woff
}

#[test]
fn a_ttf_reads_family_style_glyphs_and_characters() {
    let face = FontFace::parse(ARCHIVO.to_vec()).unwrap();
    assert_eq!(face.family, "Archivo");
    assert_eq!(face.style, "Regular");
    assert_eq!(face.format, FontFormat::TrueType);
    assert!(face.glyphs > 100);
    assert!(face.chars.contains(&'A') && face.chars.len() <= 256);
    assert!(face.version.unwrap().starts_with("Version"));
}

#[test]
fn a_woff_font_unpacks_to_the_same_face() {
    let ttf = FontFace::parse(ARCHIVO.to_vec()).unwrap();
    let woff = FontFace::parse(woff_of(ARCHIVO)).unwrap();
    assert_eq!(woff.format, FontFormat::Woff);
    assert_eq!((woff.family, woff.glyphs), (ttf.family, ttf.glyphs));
}

#[test]
fn damaged_or_unknown_fonts_explain_themselves() {
    let mut woff = woff_of(ARCHIVO);
    woff.truncate(200);
    assert_eq!(
        FontFace::parse(woff).unwrap_err(),
        "This WOFF font looks damaged."
    );
    assert!(FontFace::parse(ARCHIVO[..300].to_vec()).is_err());
    assert_eq!(font_format(b"wOF2\x00\x01"), None);
    assert_eq!(font_format(b"\x00\x01\x00\x00\x00\x00"), None);
}
