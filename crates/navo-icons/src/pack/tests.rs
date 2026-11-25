use super::*;

/// A small SVG body that differs per icon.
fn svg(name: &str) -> Vec<u8> {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0L{}"/></svg>"#,
        name.len()
    )
    .into_bytes()
}

fn icon(name: &str, brand: Option<[u8; 3]>) -> IconDraft {
    IconDraft {
        name: name.to_owned(),
        svg: svg(name),
        tags: vec![format!("word-{name}"), "shared".to_owned()],
        brand,
    }
}

/// Two sets: `lucide` with 70 icons (two chunks, given out of order) and `brands` with colours.
fn drafts() -> Vec<SetDraft> {
    let mut icons: Vec<IconDraft> = (0..70)
        .rev()
        .map(|n| icon(&format!("icon-{n:02}"), None))
        .collect();
    icons.push(icon("a", None));
    vec![
        SetDraft {
            id: "lucide".to_owned(),
            version: "1.78.0".to_owned(),
            licence: "ISC".to_owned(),
            grid: 24,
            icons,
        },
        SetDraft {
            id: "brands".to_owned(),
            version: "16.38.1".to_owned(),
            licence: "CC0-1.0".to_owned(),
            grid: 24,
            icons: vec![icon("github", Some([0x18, 0x17, 0x17])), icon("rust", None)],
        },
    ]
}

fn packed() -> Vec<u8> {
    write(&drafts()).unwrap()
}

fn read_body(pack: &Pack<'_>, set: &str, name: &str) -> Vec<u8> {
    let found = pack.find(set, name).unwrap();
    let chunk = pack.chunk(set, found.chunk).unwrap();
    body(&chunk, &found).unwrap().to_vec()
}

/// Reads every lookup of every drafted icon; any data that comes back must be the drafted data.
fn probe(bytes: &[u8]) {
    let Ok(pack) = Pack::open(bytes) else { return };
    for set in drafts() {
        let _ = pack.names(&set.id);
        let _ = pack.tags(&set.id);
        for draft in &set.icons {
            let Some(found) = pack.find(&set.id, &draft.name) else {
                continue;
            };
            let Ok(chunk) = pack.chunk(&set.id, found.chunk) else {
                continue;
            };
            if let Some(got) = body(&chunk, &found) {
                assert!(got.len() <= 64 * 1024, "a body is never over its cap");
            }
        }
    }
}

const RECORD: usize = 12;

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

#[test]
fn pack_round_trips_two_sets() {
    let bytes = packed();
    let pack = Pack::open(&bytes).unwrap();
    let ids: Vec<&str> = pack.sets().iter().map(|set| set.id.as_str()).collect();
    assert_eq!(ids, ["lucide", "brands"]);
    assert_eq!(
        pack.sets()[1],
        SetInfo {
            id: "brands".to_owned(),
            version: "16.38.1".to_owned(),
            licence: "CC0-1.0".to_owned(),
            grid: 24,
            count: 2,
        }
    );
    for set in drafts() {
        for draft in &set.icons {
            assert_eq!(
                read_body(&pack, &set.id, &draft.name),
                draft.svg,
                "{}",
                draft.name
            );
            assert_eq!(pack.find(&set.id, &draft.name).unwrap().brand, draft.brand);
        }
    }
    let chunks: Vec<u16> = ["icon-00", "icon-69"]
        .iter()
        .map(|name| pack.find("lucide", name).unwrap().chunk)
        .collect();
    assert_eq!(chunks, [0, 1], "64 bodies per chunk");
    let tags = pack.tags("brands").unwrap();
    assert_eq!(
        tags.lines().collect::<Vec<_>>(),
        ["word-github shared", "word-rust shared"]
    );
    assert_eq!(pack.tags("nope"), Err(PackError::NoSet("nope".to_owned())));
}

#[test]
fn pack_find_is_exact_and_ordered() {
    let bytes = packed();
    let pack = Pack::open(&bytes).unwrap();
    let names = pack.names("lucide");
    assert_eq!(names.len(), 71);
    assert_eq!(names[0], "a");
    assert!(
        names
            .windows(2)
            .all(|pair| pair[0].as_bytes() < pair[1].as_bytes())
    );
    assert!(pack.find("lucide", "icon-07").is_some());
    for miss in ["icon-7", "icon-070", "ICON-07", "", "b"] {
        assert_eq!(pack.find("lucide", miss), None, "{miss}");
    }
    assert_eq!(pack.find("brands", "icon-07"), None, "names are per set");
    assert_eq!(pack.find("nope", "a"), None);
    assert!(pack.names("nope").is_empty());
}

#[test]
fn pack_open_refuses_bad_magic_and_version() {
    let mut bytes = packed();
    bytes[0] = b'X';
    assert_eq!(Pack::open(&bytes).err(), Some(PackError::BadMagic));
    let mut bytes = packed();
    bytes[6..8].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(Pack::open(&bytes).err(), Some(PackError::BadVersion(2)));
    let mut bytes = packed();
    bytes[8..12].copy_from_slice(&17u32.to_le_bytes());
    assert_eq!(Pack::open(&bytes).err(), Some(PackError::OutOfRange));
}

#[test]
fn pack_open_never_panics_on_any_prefix() {
    let bytes = packed();
    for len in 0..bytes.len() {
        probe(&bytes[..len]);
    }
    assert!(
        Pack::open(&bytes[..RECORD + 79]).is_err(),
        "a cut set record is refused"
    );
}

#[test]
fn pack_open_never_panics_on_flipped_bytes() {
    let bytes = packed();
    for at in 0..bytes.len() {
        let mut broken = bytes.clone();
        broken[at] ^= 0xA5;
        probe(&broken);
    }
}

#[test]
fn pack_chunk_refuses_raw_len_over_cap() {
    let bytes = packed();
    let directory = usize::try_from(read_u32(&bytes, RECORD + 72)).unwrap();
    let raw_at = directory + 8;
    let raw = read_u32(&bytes, raw_at);
    let patched = |value: u32| {
        let mut broken = bytes.clone();
        broken[raw_at..raw_at + 4].copy_from_slice(&value.to_le_bytes());
        broken
    };
    let over = patched(1024 * 1024 + 1);
    assert_eq!(
        Pack::open(&over).unwrap().chunk("lucide", 0),
        Err(PackError::TooLarge)
    );
    for wrong in [raw - 1, raw + 1] {
        let broken = patched(wrong);
        assert!(matches!(
            Pack::open(&broken).unwrap().chunk("lucide", 0),
            Err(PackError::Zstd(_))
        ));
    }
    assert_eq!(
        Pack::open(&bytes).unwrap().chunk("lucide", 2),
        Err(PackError::OutOfRange)
    );
}

#[test]
fn pack_body_holds_its_cap() {
    let at_cap = IconDraft {
        name: "big".to_owned(),
        svg: vec![b'x'; 64 * 1024],
        tags: Vec::new(),
        brand: None,
    };
    let icons: Vec<IconDraft> = (0..16)
        .map(|n| IconDraft {
            name: format!("big-{n:02}"),
            ..at_cap.clone()
        })
        .collect();
    let set = SetDraft {
        id: "big".to_owned(),
        version: String::new(),
        licence: String::new(),
        grid: 24,
        icons,
    };
    let bytes = write(std::slice::from_ref(&set)).unwrap();
    let pack = Pack::open(&bytes).unwrap();
    assert_eq!(
        read_body(&pack, "big", "big-15").len(),
        64 * 1024,
        "a 1 MiB chunk of 64 KiB bodies reads"
    );
    let over = IconDraft {
        svg: vec![b'x'; 64 * 1024 + 1],
        ..at_cap
    };
    let set = SetDraft {
        icons: vec![over],
        ..set
    };
    assert_eq!(write(&[set]), Err(PackError::TooLarge));
    let chunk = vec![0; 70 * 1024];
    let icon = IconRef {
        chunk: 0,
        offset: 0,
        len: 64 * 1024 + 1,
        brand: None,
    };
    assert_eq!(body(&chunk, &icon), None);
    assert_eq!(
        body(
            &chunk,
            &IconRef {
                len: 64 * 1024,
                ..icon
            }
        )
        .map(<[u8]>::len),
        Some(64 * 1024)
    );
    assert_eq!(
        body(
            &chunk,
            &IconRef {
                offset: u32::MAX,
                len: 2,
                ..icon
            }
        ),
        None
    );
}

#[test]
fn pack_write_refuses_duplicate_names() {
    let mut sets = drafts();
    sets[1].icons.push(icon("rust", None));
    assert_eq!(
        write(&sets),
        Err(PackError::DuplicateName("rust".to_owned()))
    );
    let mut sets = drafts();
    sets[1].id = "lucide".to_owned();
    assert_eq!(
        write(&sets),
        Err(PackError::DuplicateName("lucide".to_owned()))
    );
}

#[test]
fn pack_write_refuses_bad_text() {
    for (id, name, tag) in [
        ("Bad Id", "a", "x"),
        ("ok", "", "x"),
        ("ok", "a", "two words"),
        ("ok", "a", ""),
    ] {
        let mut draft = icon(name, None);
        draft.tags = vec![tag.to_owned()];
        let set = SetDraft {
            id: id.to_owned(),
            version: "1".to_owned(),
            licence: "MIT".to_owned(),
            grid: 24,
            icons: vec![draft],
        };
        assert_eq!(
            write(&[set]),
            Err(PackError::BadText),
            "{id:?} {name:?} {tag:?}"
        );
    }
    assert_eq!(write(&[]), Err(PackError::OutOfRange));
}

#[test]
fn pack_write_is_deterministic() {
    let mut shuffled = drafts();
    shuffled[0].icons.reverse();
    assert_eq!(packed(), packed());
    assert_eq!(
        packed(),
        write(&shuffled).unwrap(),
        "icon order in a draft does not matter"
    );
}
