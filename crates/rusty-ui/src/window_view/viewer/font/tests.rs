use std::borrow::Cow;
use std::io::Write as _;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use flate2::Compression;
use flate2::write::ZlibEncoder;
use gpui_kit::TestAppContext;
use ttf_parser::Face;

use super::face::{FontFace, FontFormat, font_format};
use super::register;
use super::sfnt::{self, Table};

const ARCHIVO: &[u8] = include_bytes!("../../../../fonts/Archivo-Regular.ttf");
const NAME_IDS: [u16; 5] = [1, 4, 6, 16, 21];
static KEEP: AtomicBool = AtomicBool::new(false);

fn parse(bytes: Vec<u8>) -> Result<FontFace, String> {
    FontFace::parse(bytes, &KEEP)
}

fn be16(bytes: &[u8], at: usize) -> usize {
    usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]))
}

fn put32(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&u32::try_from(value).unwrap().to_be_bytes());
}

struct Packed {
    tag: u32,
    stored: Vec<u8>,
    length: usize,
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn packed_tables(sfnt: &[u8]) -> Vec<Packed> {
    let (_, tables) = sfnt::tables(sfnt, 0).unwrap();
    tables
        .iter()
        .map(|table| {
            let packed = zlib(&table.data);
            let stored = if packed.len() < table.data.len() {
                packed
            } else {
                table.data.to_vec()
            };
            Packed {
                tag: table.tag,
                stored,
                length: table.data.len(),
            }
        })
        .collect()
}

fn woff_from(flavor: &[u8], tables: &[Packed]) -> Vec<u8> {
    let start = 44 + 20 * tables.len();
    let mut directory = Vec::new();
    let mut data = Vec::new();
    for table in tables {
        directory.extend_from_slice(&table.tag.to_be_bytes());
        put32(&mut directory, start + data.len());
        put32(&mut directory, table.stored.len());
        put32(&mut directory, table.length);
        put32(&mut directory, 0);
        data.extend_from_slice(&table.stored);
        data.resize(data.len().next_multiple_of(4), 0);
    }
    let mut woff = b"wOFF".to_vec();
    woff.extend_from_slice(flavor);
    put32(&mut woff, start + data.len());
    woff.extend_from_slice(&u16::try_from(tables.len()).unwrap().to_be_bytes());
    woff.extend_from_slice(&[0, 0]);
    put32(&mut woff, ARCHIVO.len());
    woff.resize(44, 0);
    woff.extend_from_slice(&directory);
    woff.extend_from_slice(&data);
    woff
}

fn woff_of(sfnt: &[u8]) -> Vec<u8> {
    woff_from(&sfnt[..4], &packed_tables(sfnt))
}

fn with_table(sfnt: &[u8], tag: &[u8; 4], data: Vec<u8>) -> Vec<u8> {
    let (flavor, mut tables) = sfnt::tables(sfnt, 0).unwrap();
    let tag = u32::from_be_bytes(*tag);
    tables.retain(|table| table.tag != tag);
    tables.push(Table {
        tag,
        data: Cow::Owned(data),
    });
    sfnt::write(flavor, tables).unwrap()
}

fn name_table(sfnt: &[u8]) -> Vec<u8> {
    let (_, tables) = sfnt::tables(sfnt, 0).unwrap();
    let name = u32::from_be_bytes(*b"name");
    let table = tables.iter().find(|table| table.tag == name).unwrap();
    table.data.to_vec()
}

fn family_names(sfnt: &[u8]) -> Vec<(u16, String)> {
    let face = Face::parse(sfnt, 0).unwrap();
    face.names()
        .into_iter()
        .filter(|name| NAME_IDS.contains(&name.name_id))
        .filter_map(|name| Some((name.name_id, name.to_string()?)))
        .collect()
}

#[test]
fn a_ttf_reads_family_style_glyphs_and_characters() {
    let face = parse(ARCHIVO.to_vec()).unwrap();
    assert_eq!(face.family, "Archivo");
    assert_eq!(face.style, "Regular");
    assert_eq!(face.format, FontFormat::TrueType);
    assert!(face.glyphs > 100);
    assert!(face.chars.contains(&'A') && face.chars.len() <= 256);
    assert!(face.version.unwrap().starts_with("Version"));
}

#[test]
fn a_woff_font_unpacks_to_the_same_face() {
    let ttf = parse(ARCHIVO.to_vec()).unwrap();
    let woff = parse(woff_of(ARCHIVO)).unwrap();
    assert_eq!(woff.format, FontFormat::Woff);
    assert_eq!((woff.family, woff.glyphs), (ttf.family, ttf.glyphs));
}

#[test]
fn damaged_or_unknown_fonts_explain_themselves() {
    let mut woff = woff_of(ARCHIVO);
    woff.truncate(200);
    assert_eq!(parse(woff).unwrap_err(), "This WOFF font looks damaged.");
    assert!(parse(ARCHIVO[..300].to_vec()).is_err());
    assert_eq!(font_format(b"wOF2\x00\x01"), None);
    assert_eq!(font_format(b"\x00\x01\x00\x00\x00\x00"), None);
}

#[test]
fn a_woff_with_too_many_tables_is_refused() {
    let mut tables = packed_tables(ARCHIVO);
    let last = tables.last().unwrap();
    let copies: Vec<Packed> = (0..129)
        .map(|_| Packed {
            tag: u32::from_be_bytes(*b"zzzz"),
            stored: last.stored.clone(),
            length: last.length,
        })
        .collect();
    tables.extend(copies);
    let woff = woff_from(&ARCHIVO[..4], &tables);
    assert_eq!(parse(woff).unwrap_err(), "This WOFF font looks damaged.");
}

#[test]
fn a_woff_declaring_over_64_mb_is_refused_before_inflating() {
    let mut oversized = woff_of(ARCHIVO);
    oversized[16..20].copy_from_slice(&(100u32 << 20).to_be_bytes());
    assert_eq!(
        parse(oversized).unwrap_err(),
        "This WOFF font looks damaged."
    );
    let length = 33 << 20;
    let zeros = zlib(&vec![0; length]);
    let mut tables = packed_tables(ARCHIVO);
    for tag in [b"zzz1", b"zzz2"] {
        tables.push(Packed {
            tag: u32::from_be_bytes(*tag),
            stored: zeros.clone(),
            length,
        });
    }
    assert_eq!(
        parse(woff_from(&ARCHIVO[..4], &tables)).unwrap_err(),
        "This WOFF font looks damaged."
    );
}

fn cmap_mapping_everything() -> Vec<u8> {
    let mut cmap = Vec::new();
    for value in [0u16, 1, 3, 10] {
        cmap.extend_from_slice(&value.to_be_bytes());
    }
    cmap.extend_from_slice(&12u32.to_be_bytes());
    cmap.extend_from_slice(&12u16.to_be_bytes());
    cmap.extend_from_slice(&0u16.to_be_bytes());
    for value in [28u32, 0, 1, 0, u32::MAX, 1] {
        cmap.extend_from_slice(&value.to_be_bytes());
    }
    cmap
}

#[test]
fn a_cmap_group_spanning_every_code_point_stops_at_the_grid_size() {
    let font = with_table(ARCHIVO, b"cmap", cmap_mapping_everything());
    let face = parse(font).unwrap();
    assert_eq!(face.chars.len(), 256);
    assert_eq!(face.chars[0], '!');
}

fn cmap_of(records: u16, subtable: &[u8]) -> Vec<u8> {
    let mut cmap = Vec::new();
    cmap.extend_from_slice(&0u16.to_be_bytes());
    cmap.extend_from_slice(&records.to_be_bytes());
    let offset = 4 + 8 * u32::from(records);
    for _ in 0..records {
        cmap.extend_from_slice(&3u16.to_be_bytes());
        cmap.extend_from_slice(&10u16.to_be_bytes());
        cmap.extend_from_slice(&offset.to_be_bytes());
    }
    cmap.extend_from_slice(subtable);
    cmap
}

fn groups_subtable(format: u16, groups: &[[u32; 3]]) -> Vec<u8> {
    let count = u32::try_from(groups.len()).unwrap();
    let mut subtable = Vec::new();
    subtable.extend_from_slice(&format.to_be_bytes());
    subtable.extend_from_slice(&0u16.to_be_bytes());
    subtable.extend_from_slice(&(16 + 12 * count).to_be_bytes());
    subtable.extend_from_slice(&0u32.to_be_bytes());
    subtable.extend_from_slice(&count.to_be_bytes());
    for value in groups.iter().flatten() {
        subtable.extend_from_slice(&value.to_be_bytes());
    }
    subtable
}

fn format_13_at_the_last_code_point(groups: usize) -> Vec<u8> {
    groups_subtable(13, &vec![[0x10_FFFF, 0x10_FFFF, 1]; groups])
}

#[test]
fn a_hostile_cmap_is_walked_once_and_quickly() {
    let subtable = format_13_at_the_last_code_point(2_000);
    for records in [1, u16::MAX] {
        let font = with_table(ARCHIVO, b"cmap", cmap_of(records, &subtable));
        let started = Instant::now();
        let face = parse(font).unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{records} records"
        );
        assert_eq!(face.chars, ['\u{10FFFF}']);
    }
}

#[test]
fn a_cancelled_parse_stops_walking_the_cmap() {
    let mut groups = vec![[0xD800, 0xDFFF, 1]; 500];
    groups.push([0x41, 0x5A, 1]);
    let cmap = cmap_of(1, &groups_subtable(12, &groups));
    let font = with_table(ARCHIVO, b"cmap", cmap);
    assert_eq!(parse(font.clone()).unwrap().chars.len(), 26);
    let cancelled = AtomicBool::new(true);
    assert!(FontFace::parse(font, &cancelled).unwrap().chars.is_empty());
}

#[test]
fn the_preview_registers_under_a_private_family_and_keeps_the_real_name() {
    let face = parse(ARCHIVO.to_vec()).unwrap();
    assert_eq!(face.family, "Archivo");
    let preview = face.preview.unwrap();
    assert!(preview.family.starts_with("Navo Preview "));
    let names = family_names(&preview.bytes);
    assert!(
        names
            .iter()
            .any(|(id, name)| *id == 1 && *name == preview.family)
    );
    assert!(
        names
            .iter()
            .any(|(id, name)| *id == 16 && *name == preview.family)
    );
    assert!(
        names.iter().all(|(_, name)| !name.contains("Archivo")),
        "{names:?}"
    );
}

#[test]
fn different_fonts_get_different_private_families() {
    let mut other = ARCHIVO.to_vec();
    let last = other.len() - 1;
    other[last] ^= 1;
    let first = parse(ARCHIVO.to_vec()).unwrap().preview.unwrap();
    let second = parse(other).unwrap().preview.unwrap();
    assert_ne!(first.family, second.family);
}

#[test]
fn a_name_table_that_cannot_be_rewritten_leaves_no_preview() {
    let mut name = name_table(ARCHIVO);
    let count = be16(&name, 2);
    let record = (0..count)
        .rev()
        .map(|index| 6 + 12 * index)
        .find(|at| !NAME_IDS.contains(&(be16(&name, at + 6) as u16)))
        .unwrap();
    name[record + 8..record + 10].copy_from_slice(&u16::MAX.to_be_bytes());
    let face = parse(with_table(ARCHIVO, b"name", name)).unwrap();
    assert_eq!(face.family, "Archivo");
    assert!(face.preview.is_none());
}

#[gpui_kit::test]
fn viewing_the_same_font_again_does_not_register_it_again(cx: &mut TestAppContext) {
    let first = parse(ARCHIVO.to_vec()).unwrap().preview.unwrap();
    let again = parse(ARCHIVO.to_vec()).unwrap().preview.unwrap();
    cx.update(|cx| {
        assert_eq!(register(first, cx), Ok(true));
        assert_eq!(register(again, cx), Ok(false));
    });
}
