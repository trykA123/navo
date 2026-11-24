use super::*;

fn table(text: &str) -> toml::Table {
    toml::from_str(text).unwrap()
}

fn render(fields: &IconFields) -> toml::Table {
    let value = match fields {
        IconFields::Text(text) => toml::Value::String(text.clone()),
        IconFields::Table(fields) => {
            let mut table = toml::Table::new();
            for (key, value) in fields {
                let value = match value {
                    FieldValue::Text(text) => toml::Value::String(text.clone()),
                    FieldValue::Flag(flag) => toml::Value::Boolean(*flag),
                };
                table.insert((*key).to_owned(), value);
            }
            toml::Value::Table(table)
        }
    };
    let mut document = toml::Table::new();
    document.insert("icon".to_owned(), value);
    document
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
fn spec_round_trips_every_form_through_fields() {
    let specs = [
        IconSpec::Set {
            set: "lucide".to_owned(),
            name: "file-pen".to_owned(),
            tone: None,
        },
        IconSpec::Set {
            set: "tabler".to_owned(),
            name: "terminal-2".to_owned(),
            tone: Some(Tone::Warn),
        },
        IconSpec::Image {
            file: "my-icon-12345678.png".to_owned(),
        },
        IconSpec::Image {
            file: "my-icon-12345678.svg".to_owned(),
        },
        IconSpec::Brand {
            name: "github".to_owned(),
            colour: BrandColour::Theme,
        },
        IconSpec::Brand {
            name: "github".to_owned(),
            colour: BrandColour::Brand,
        },
        IconSpec::Svg {
            file: "my-icon-12345678.svg".to_owned(),
            mono: true,
        },
        IconSpec::Svg {
            file: "my-icon-12345678.svg".to_owned(),
            mono: false,
        },
        IconSpec::Letters {
            letters: "C".to_owned(),
            shape: Shape::Soft,
            tone: Tone::Accent,
        },
        IconSpec::Letters {
            letters: "AB".to_owned(),
            shape: Shape::Circle,
            tone: Tone::Ok,
        },
        IconSpec::Glyph {
            glyph: "hash".to_owned(),
            tone: Tone::Text,
            dot: None,
        },
        IconSpec::Glyph {
            glyph: "hash".to_owned(),
            tone: Tone::KindCode,
            dot: Some(Tone::Ok),
        },
    ];
    for spec in specs {
        let document = render(&to_fields(&spec));
        let icon = document.get("icon").unwrap();
        let parsed = match icon {
            toml::Value::String(text) => parse_text(text).unwrap(),
            toml::Value::Table(table) => parse_table(table).unwrap(),
            other => panic!("icon field is {other:?}"),
        };
        assert_eq!(parsed, spec);
    }
}
