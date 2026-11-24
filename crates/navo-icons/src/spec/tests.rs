use super::*;

fn table(text: &str) -> toml::Table {
    toml::from_str(text).unwrap()
}

fn parse_document(text: &str) -> IconSpec {
    let document = table(text);
    match document.get("icon").unwrap() {
        toml::Value::String(text) => parse_text(text).unwrap(),
        toml::Value::Table(table) => parse_table(table).unwrap(),
        other => panic!("icon field is {other:?}"),
    }
}

fn quote(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
}

fn render(fields: &IconFields) -> String {
    match fields {
        IconFields::Text(text) => format!("icon = {}\n", quote(text)),
        IconFields::Table(fields) => {
            let entries: Vec<String> = fields
                .iter()
                .map(|(key, value)| {
                    let value = match value {
                        FieldValue::Text(text) => quote(text),
                        FieldValue::Flag(flag) => flag.to_string(),
                    };
                    format!("{key} = {value}")
                })
                .collect();
            format!("icon = {{ {} }}\n", entries.join(", "))
        }
    }
}

fn keys(fields: &IconFields) -> Vec<&'static str> {
    match fields {
        IconFields::Text(_) => Vec::new(),
        IconFields::Table(fields) => fields.iter().map(|(key, _)| *key).collect(),
    }
}

#[test]
fn spec_reads_set_and_name() {
    assert_eq!(
        parse_text("lucide:file-pen").unwrap(),
        IconSpec::Set {
            set: "lucide".to_owned(),
            name: "file-pen".to_owned(),
            tone: None,
        }
    );
    assert!(parse_text("lucide").is_err());
    assert!(parse_text("Lucide:file-pen").is_err());
    assert!(parse_text("lucide:File").is_err());
}

#[test]
fn spec_reads_image_file_and_refuses_paths() {
    assert_eq!(
        parse_text("image:my-icon-12345678.png").unwrap(),
        IconSpec::Image {
            file: "my-icon-12345678.png".to_owned(),
        }
    );
    assert_eq!(
        parse_text("image:my-icon-12345678.svg").unwrap(),
        IconSpec::Image {
            file: "my-icon-12345678.svg".to_owned(),
        }
    );
    for refused in [
        "image:../x.png",
        "image:a/b-12345678.png",
        "image:X-12345678.png",
        "image:a-1234567.png",
    ] {
        assert!(parse_text(refused).is_err(), "{refused} must be refused");
    }
}

#[test]
fn spec_brand_defaults_to_theme_colour() {
    let expected = IconSpec::Brand {
        name: "github".to_owned(),
        colour: BrandColour::Theme,
    };
    assert_eq!(parse_text("brands:github").unwrap(), expected);
    let theme = table(
        r#"
set = "brands"
name = "github"
"#,
    );
    assert_eq!(parse_table(&theme).unwrap(), expected);
    let brand = table(
        r#"
set = "brands"
name = "github"
colour = "brand"
"#,
    );
    assert_eq!(
        parse_table(&brand).unwrap(),
        IconSpec::Brand {
            name: "github".to_owned(),
            colour: BrandColour::Brand,
        }
    );
}

#[test]
fn spec_letters_upper_case_and_cap_two() {
    let letters = table(r#"letters = "ab""#);
    assert_eq!(
        parse_table(&letters).unwrap(),
        IconSpec::Letters {
            letters: "AB".to_owned(),
            shape: Shape::Soft,
            tone: Tone::Accent,
        }
    );
    for refused in ["letters = \"\"", "letters = \"abc\"", "letters = \"a!\""] {
        assert!(
            parse_table(&table(refused)).is_err(),
            "{refused} must be refused"
        );
    }
}

#[test]
fn spec_refuses_mixed_forms() {
    let mixed = table(
        r#"
letters = "C"
set = "lucide"
name = "file-pen"
"#,
    );
    assert_eq!(
        parse_table(&mixed).unwrap_err().to_string(),
        "icon mixes `letters` and `set`"
    );
}

#[test]
fn spec_refuses_unknown_tone() {
    let unknown = table(
        r#"
letters = "C"
tone = "neon"
"#,
    );
    assert_eq!(
        parse_table(&unknown).unwrap_err().to_string(),
        "unknown icon tone `neon`"
    );
}

#[test]
fn spec_refuses_unknown_key() {
    let unknown = table(
        r#"
set = "lucide"
name = "file-pen"
colour = "brand"
"#,
    );
    assert_eq!(
        parse_table(&unknown).unwrap_err().to_string(),
        "unknown icon key `colour`"
    );
}

#[test]
fn spec_names_unknown_key_without_a_form() {
    let unknown = table(r#"unknown = "hash""#);
    assert_eq!(
        parse_table(&unknown).unwrap_err().to_string(),
        "unknown icon key `unknown`"
    );
}

#[test]
fn spec_refuses_wrong_type() {
    let wrong = table(
        r#"
letters = "C"
tone = 3
"#,
    );
    assert_eq!(
        parse_table(&wrong).unwrap_err().to_string(),
        "icon `tone` must be text"
    );
}

#[test]
fn spec_set_named_image_keeps_table_form() {
    let spec = parse_document("icon = { set = \"image\", name = \"foo\" }\n");
    assert_eq!(
        to_fields(&spec),
        IconFields::Table(vec![
            ("set", FieldValue::Text("image".to_owned())),
            ("name", FieldValue::Text("foo".to_owned())),
        ])
    );
    assert_eq!(parse_document(&render(&to_fields(&spec))), spec);
}

#[test]
fn spec_fields_keep_canonical_order_and_omit_defaults() {
    let set_with_tone = IconSpec::Set {
        set: "tabler".to_owned(),
        name: "terminal-2".to_owned(),
        tone: Some(Tone::Warn),
    };
    assert_eq!(
        keys(&to_fields(&set_with_tone)),
        vec!["set", "name", "tone"]
    );
    let brand_with_colour = IconSpec::Brand {
        name: "github".to_owned(),
        colour: BrandColour::Brand,
    };
    assert_eq!(
        keys(&to_fields(&brand_with_colour)),
        vec!["set", "name", "colour"]
    );
    let svg_default = IconSpec::Svg {
        file: "my-icon-12345678.svg".to_owned(),
        mono: true,
    };
    assert_eq!(keys(&to_fields(&svg_default)), vec!["svg"]);
    let svg_colour = IconSpec::Svg {
        file: "my-icon-12345678.svg".to_owned(),
        mono: false,
    };
    assert_eq!(keys(&to_fields(&svg_colour)), vec!["svg", "mono"]);
    let letters_default = IconSpec::Letters {
        letters: "C".to_owned(),
        shape: Shape::Soft,
        tone: Tone::Accent,
    };
    assert_eq!(keys(&to_fields(&letters_default)), vec!["letters"]);
    let letters_full = IconSpec::Letters {
        letters: "AB".to_owned(),
        shape: Shape::Circle,
        tone: Tone::Ok,
    };
    assert_eq!(
        keys(&to_fields(&letters_full)),
        vec!["letters", "shape", "tone"]
    );
    let glyph_default = IconSpec::Glyph {
        glyph: "hash".to_owned(),
        tone: Tone::Text,
        dot: None,
    };
    assert_eq!(keys(&to_fields(&glyph_default)), vec!["glyph"]);
    let glyph_full = IconSpec::Glyph {
        glyph: "hash".to_owned(),
        tone: Tone::KindCode,
        dot: Some(Tone::Ok),
    };
    assert_eq!(keys(&to_fields(&glyph_full)), vec!["glyph", "tone", "dot"]);
    let set_image = IconSpec::Set {
        set: "image".to_owned(),
        name: "foo".to_owned(),
        tone: None,
    };
    assert_eq!(keys(&to_fields(&set_image)), vec!["set", "name"]);
    assert_eq!(
        to_fields(&IconSpec::Brand {
            name: "github".to_owned(),
            colour: BrandColour::Theme,
        }),
        IconFields::Text("brands:github".to_owned())
    );
    assert_eq!(
        to_fields(&IconSpec::Set {
            set: "lucide".to_owned(),
            name: "file-pen".to_owned(),
            tone: None,
        }),
        IconFields::Text("lucide:file-pen".to_owned())
    );
}

#[test]
fn spec_round_trips_every_form_through_fields() {
    let documents = [
        "icon = \"lucide:file-pen\"\n",
        "icon = { set = \"tabler\", name = \"terminal-2\", tone = \"warn\" }\n",
        "icon = \"image:my-icon-12345678.png\"\n",
        "icon = \"image:my-icon-12345678.svg\"\n",
        "icon = \"brands:github\"\n",
        "icon = { set = \"brands\", name = \"github\", colour = \"brand\" }\n",
        "icon = { svg = \"my-icon-12345678.svg\" }\n",
        "icon = { svg = \"my-icon-12345678.svg\", mono = false }\n",
        "icon = { letters = \"C\" }\n",
        "icon = { letters = \"AB\", shape = \"circle\", tone = \"ok\" }\n",
        "icon = { glyph = \"hash\" }\n",
        "icon = { glyph = \"hash\", tone = \"kind-code\", dot = \"ok\" }\n",
        "icon = { set = \"image\", name = \"foo\" }\n",
    ];
    for document in documents {
        let spec = parse_document(document);
        let written = render(&to_fields(&spec));
        let reparsed = parse_document(&written);
        assert_eq!(reparsed, spec, "{document}");
    }
}

#[test]
fn spec_bounds_error_diagnostics() {
    let long = "x".repeat(100);
    let error = parse_text(&long).unwrap_err().to_string();
    assert!(error.contains(&"x".repeat(64)));
    assert!(!error.contains(&"x".repeat(65)));

    let letters = "a".repeat(100);
    let input = format!("letters = \"{letters}\"");
    let error = parse_table(&table(&input)).unwrap_err().to_string();
    assert!(error.contains(&"a".repeat(64)));
    assert!(!error.contains(&"a".repeat(65)));
}

#[test]
fn spec_enforces_set_name_and_stem_limits() {
    for (length, allowed) in [(23, true), (24, true), (25, false)] {
        let set = format!("a{}", "b".repeat(length - 1));
        assert_eq!(
            parse_text(&format!("{set}:x")).is_ok(),
            allowed,
            "set length {length}"
        );
    }
    for (length, allowed) in [(63, true), (64, true), (65, false)] {
        let name = format!("a{}", "b".repeat(length - 1));
        assert_eq!(
            parse_text(&format!("lucide:{name}")).is_ok(),
            allowed,
            "name length {length}"
        );
    }
    for (length, allowed) in [(31, true), (32, true), (33, false)] {
        let stem = format!("a{}", "b".repeat(length - 1));
        let file = format!("{stem}-12345678.png");
        assert_eq!(
            parse_text(&format!("image:{file}")).is_ok(),
            allowed,
            "stem length {length}"
        );
    }
}

#[test]
fn spec_svg_table_refuses_png_suffix() {
    let png = table(r#"svg = "a-12345678.png""#);
    assert_eq!(
        parse_table(&png).unwrap_err().to_string(),
        "invalid icon file `a-12345678.png`"
    );
}

#[test]
fn spec_refuses_non_boolean_mono() {
    let mono = table(
        r#"
svg = "a-12345678.svg"
mono = "yes"
"#,
    );
    assert_eq!(
        parse_table(&mono).unwrap_err().to_string(),
        "icon `mono` must be a flag"
    );
}
