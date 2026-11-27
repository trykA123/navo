use super::*;

/// Packs the named entries into an in-memory gzipped tar.
fn tgz(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_path(name).unwrap();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, *bytes).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

/// Builds a file map from the given entries.
fn map(entries: &[(&str, &[u8])]) -> BTreeMap<String, Vec<u8>> {
    entries
        .iter()
        .map(|(name, bytes)| ((*name).to_owned(), bytes.to_vec()))
        .collect()
}

/// A minimal valid SVG with one visible stroke path.
fn svg(shape: &str) -> Vec<u8> {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">{shape}</svg>"#)
        .into_bytes()
}

const PATH: &str = r##"<path d="M2 2L20 20" fill="none" stroke="#000" stroke-width="2"/>"##;

/// A spec with fixed metadata and the given reader.
fn spec(reader: Reader) -> SourceSpec {
    SourceSpec {
        id: "test".to_owned(),
        version: "1.0.0".to_owned(),
        licence: "MIT".to_owned(),
        grid: 24,
        reader,
    }
}

#[test]
fn sources_nodes_build_stroke_documents() {
    let nodes = br#"{"Arrow":[["path",{"d":"M5 12h14"}],["line",{"x1":"12","y1":"5","x2":"12","y2":"19"}]],"Circle":[["circle",{"cx":"12","cy":"12","r":"9"}]]}"#;
    let files = map(&[("package/icon-nodes.json", nodes)]);
    let reader = Reader::Nodes {
        file: "package/icon-nodes.json".to_owned(),
        style: Style::Stroke,
        tags: None,
    };

    let set = build_set(&files, &spec(reader)).unwrap();

    assert_eq!(set.icons.len(), 2);
    let names: Vec<&str> = set.icons.iter().map(|icon| icon.name.as_str()).collect();
    assert_eq!(names, ["arrow", "circle"]);
    for icon in &set.icons {
        let text = std::str::from_utf8(&icon.svg).unwrap();
        assert!(text.contains(r#"viewBox="0 0 24 24""#), "{text}");
        assert!(text.contains("<path"), "{text}");
        assert!(text.contains(r##"stroke="#000000""##), "{text}");
    }
}

#[test]
fn sources_simple_icons_skip_any_license_entry() {
    let data = br##"[
        {"title":"Keep Me","slug":"keep-me","hex":"#112233"},
        {"title":"Drop Cc0","slug":"drop-cc0","hex":"AABBCC","license":{"type":"CC0-1.0"}},
        {"title":"Drop Custom","slug":"drop-custom","license":{"type":"custom"}},
        {"title":"Drop Null","slug":"drop-null","hex":"000000","license":null}
    ]"##;
    let files = map(&[
        ("package/data/simple-icons.json", data),
        ("package/icons/keep-me.svg", &svg(PATH)),
        ("package/icons/drop-cc0.svg", &svg(PATH)),
        ("package/icons/drop-custom.svg", &svg(PATH)),
        ("package/icons/drop-null.svg", &svg(PATH)),
    ]);

    let set = build_set(&files, &spec(Reader::SimpleIcons)).unwrap();

    assert_eq!(set.icons.len(), 1);
    assert_eq!(set.icons[0].name, "keep-me");
    assert_eq!(set.icons[0].brand, Some([0x11, 0x22, 0x33]));
    assert!(!set.icons.iter().any(|icon| icon.name == "drop-null"));
}

#[test]
fn sources_tags_come_from_title_and_aliases() {
    let data = br#"[{"title":"Some Fast Icon","slug":"fast",
        "aliases":{"aka":["Quick Arrow","quick"]},"hex":"010203"}]"#;
    let files = map(&[
        ("data/simple-icons.json", data),
        ("icons/fast.svg", &svg(PATH)),
    ]);

    let set = build_set(&files, &spec(Reader::SimpleIcons)).unwrap();

    assert_eq!(
        set.icons[0].tags,
        vec!["some", "fast", "icon", "quick", "arrow"]
    );
}

#[test]
fn sources_svg_dir_strips_suffix_and_skips() {
    let files = map(&[
        ("assets/regular/arrow_left.svg", &svg(PATH)),
        ("assets/regular/arrow_left-fill.svg", &svg(PATH)),
        ("assets/regular/nested/dot.svg", &svg(PATH)),
        ("assets/regular/readme.txt", b"not an svg"),
    ]);
    let reader = Reader::SvgDir {
        dir: "assets/regular".to_owned(),
        suffix: ".svg".to_owned(),
        strip: "underscore".to_owned(),
        skip_suffix: Some("-fill.svg".to_owned()),
    };

    let set = build_set(&files, &spec(reader)).unwrap();

    let names: Vec<&str> = set.icons.iter().map(|icon| icon.name.as_str()).collect();
    assert_eq!(names, ["arrow-left"]);
}

#[test]
fn sources_svg_dir_skips_nested_subdirectories() {
    let files = map(&[
        ("package/icons/mail_24_regular.svg", &svg(PATH)),
        ("package/icons/ar/mail_24_regular.svg", &svg(PATH)),
        ("package/icons/de/nested/mail_24_regular.svg", &svg(PATH)),
    ]);
    let reader = Reader::SvgDir {
        dir: "package/icons".to_owned(),
        suffix: "_24_regular.svg".to_owned(),
        strip: "underscore".to_owned(),
        skip_suffix: None,
    };

    let set = build_set(&files, &spec(reader)).unwrap();

    let names: Vec<&str> = set.icons.iter().map(|icon| icon.name.as_str()).collect();
    assert_eq!(names, ["mail"]);
}

#[test]
fn sources_build_set_lenient_skips_invalid_names_and_broken_svgs() {
    let long_name = "a".repeat(65);
    let long_path = format!("package/svg/{long_name}.svg");
    let files = map(&[
        ("package/svg/good.svg", &svg(PATH)),
        ("package/svg/.svg", &svg(PATH)),
        (&long_path, &svg(PATH)),
        ("package/svg/broken.svg", b"<svg>not closed"),
        ("package/svg/nested/ignored.svg", &svg(PATH)),
    ]);
    let reader = Reader::SvgDir {
        dir: "package/svg".to_owned(),
        suffix: ".svg".to_owned(),
        strip: String::new(),
        skip_suffix: None,
    };

    let (set, skipped) = build_set_lenient(&files, &spec(reader)).unwrap();

    let names: Vec<&str> = set.icons.iter().map(|icon| icon.name.as_str()).collect();
    assert_eq!(names, ["good"]);
    assert_eq!(skipped, ["", long_name.as_str(), "broken"]);
}

#[test]
fn sources_tarball_refuses_entry_over_8_mib() {
    let ok_large = vec![b'x'; 2 * 1024 * 1024];
    let ok_archive = tgz(&[("package/index.json", ok_large.as_slice())]);
    assert!(read_tarball(&ok_archive, &|_| true).is_ok());

    let big = vec![b'x'; 8 * 1024 * 1024 + 1];
    let archive = tgz(&[("package/big.svg", big.as_slice())]);

    assert_eq!(
        read_tarball(&archive, &|_| true),
        Err(SourceError::TooLarge)
    );
}

#[test]
fn sources_tarball_reads_only_wanted_names() {
    let archive = tgz(&[
        ("./package/a.svg", &svg(PATH)),
        ("package/readme.txt", b"skip me"),
        ("package/b.svg", &svg(PATH)),
    ]);
    let wanted = |name: &str| name.starts_with("package/") && name.ends_with(".svg");

    let files = read_tarball(&archive, &wanted).unwrap();

    let names: Vec<&str> = files.keys().map(String::as_str).collect();
    assert_eq!(names, ["package/a.svg", "package/b.svg"]);
}
