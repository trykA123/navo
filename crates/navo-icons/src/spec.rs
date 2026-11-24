//! Parses and writes every icon form: user text, a TOML table and the fields an editor writes.

/// A theme tone an icon can be drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// The theme accent.
    Accent,
    /// The success colour.
    Ok,
    /// The warning colour.
    Warn,
    /// The folder colour.
    Folder,
    /// The code-file kind colour.
    KindCode,
    /// The auto-detected file kind colour.
    KindAuto,
    /// The image-file kind colour.
    KindImage,
    /// The archive-file kind colour.
    KindArchive,
    /// The document-file kind colour.
    KindDoc,
    /// The application-file kind colour.
    KindApp,
    /// The muted text colour.
    Muted,
    /// The normal text colour.
    Text,
}

impl Tone {
    /// The kebab-case name used in the file.
    pub fn name(self) -> &'static str {
        match self {
            Tone::Accent => "accent",
            Tone::Ok => "ok",
            Tone::Warn => "warn",
            Tone::Folder => "folder",
            Tone::KindCode => "kind-code",
            Tone::KindAuto => "kind-auto",
            Tone::KindImage => "kind-image",
            Tone::KindArchive => "kind-archive",
            Tone::KindDoc => "kind-doc",
            Tone::KindApp => "kind-app",
            Tone::Muted => "muted",
            Tone::Text => "text",
        }
    }

    /// The tone with that kebab-case name.
    pub fn named(name: &str) -> Option<Tone> {
        Some(match name {
            "accent" => Tone::Accent,
            "ok" => Tone::Ok,
            "warn" => Tone::Warn,
            "folder" => Tone::Folder,
            "kind-code" => Tone::KindCode,
            "kind-auto" => Tone::KindAuto,
            "kind-image" => Tone::KindImage,
            "kind-archive" => Tone::KindArchive,
            "kind-doc" => Tone::KindDoc,
            "kind-app" => Tone::KindApp,
            "muted" => Tone::Muted,
            "text" => Tone::Text,
            _ => return None,
        })
    }
}

/// Every tone, in the order the file lists them.
pub const TONES: [Tone; 12] = [
    Tone::Accent,
    Tone::Ok,
    Tone::Warn,
    Tone::Folder,
    Tone::KindCode,
    Tone::KindAuto,
    Tone::KindImage,
    Tone::KindArchive,
    Tone::KindDoc,
    Tone::KindApp,
    Tone::Muted,
    Tone::Text,
];

/// The background shape of a letters icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A softly tinted rounded square.
    Soft,
    /// A filled circle.
    Circle,
    /// An outlined rounded square.
    Outline,
    /// A solid coloured rounded square.
    Solid,
    /// Letters alone, with no background.
    None,
}

impl Shape {
    /// The name used in the file.
    pub fn name(self) -> &'static str {
        match self {
            Shape::Soft => "soft",
            Shape::Circle => "circle",
            Shape::Outline => "outline",
            Shape::Solid => "solid",
            Shape::None => "none",
        }
    }

    /// The shape with that name.
    pub fn named(name: &str) -> Option<Shape> {
        Some(match name {
            "soft" => Shape::Soft,
            "circle" => Shape::Circle,
            "outline" => Shape::Outline,
            "solid" => Shape::Solid,
            "none" => Shape::None,
            _ => return None,
        })
    }
}

/// Whether a brand icon uses its own colour or the theme text colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrandColour {
    /// The theme text colour.
    Theme,
    /// The brand's own colour.
    Brand,
}

impl BrandColour {
    /// The name used in the file.
    pub fn name(self) -> &'static str {
        match self {
            BrandColour::Theme => "theme",
            BrandColour::Brand => "brand",
        }
    }

    /// The colour mode with that name.
    pub fn named(name: &str) -> Option<BrandColour> {
        Some(match name {
            "theme" => BrandColour::Theme,
            "brand" => BrandColour::Brand,
            _ => return None,
        })
    }
}

/// One icon as the button bar understands it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconSpec {
    /// An icon from a bundled or downloaded set.
    Set {
        /// The set id.
        set: String,
        /// The icon name inside the set.
        name: String,
        /// An optional theme tone.
        tone: Option<Tone>,
    },
    /// A brand icon from the `brands` set.
    Brand {
        /// The brand name inside the set.
        name: String,
        /// Whether the brand colour or the theme colour is drawn.
        colour: BrandColour,
    },
    /// An imported image file.
    Image {
        /// The stored file name.
        file: String,
    },
    /// An imported SVG file that may be drawn as a tinted mask.
    Svg {
        /// The stored file name.
        file: String,
        /// Whether the SVG is drawn as a single-colour mask.
        mono: bool,
    },
    /// One or two letters on a shape.
    Letters {
        /// The upper-cased letters.
        letters: String,
        /// The background shape.
        shape: Shape,
        /// The letter colour.
        tone: Tone,
    },
    /// A Lucide glyph with an optional status dot.
    Glyph {
        /// The Lucide icon name.
        glyph: String,
        /// The glyph colour.
        tone: Tone,
        /// An optional dot colour.
        dot: Option<Tone>,
    },
}

/// Why an icon spec could not be read.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SpecError(pub String);

/// One value an editor writes for a field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldValue {
    /// A text value.
    Text(String),
    /// A flag value.
    Flag(bool),
}

/// The `icon = …` form an editor writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconFields {
    /// A `"set:name"`, `"brands:name"` or `"image:file"` string.
    Text(String),
    /// An inline table, with keys in file order.
    Table(Vec<(&'static str, FieldValue)>),
}

/// Parses the `icon = "…"` string form.
pub fn parse_text(text: &str) -> Result<IconSpec, SpecError> {
    if let Some(file) = text.strip_prefix("image:") {
        check_file(file, &[".png", ".svg"])?;
        return Ok(IconSpec::Image {
            file: file.to_owned(),
        });
    }
    let Some((set, name)) = text.split_once(':') else {
        return Err(SpecError(format!(
            "icon `{}` is not `set:name` or `image:file`",
            excerpt(text)
        )));
    };
    if set == "brands" {
        check_name(name)?;
        return Ok(IconSpec::Brand {
            name: name.to_owned(),
            colour: BrandColour::Theme,
        });
    }
    check_set(set)?;
    check_name(name)?;
    Ok(IconSpec::Set {
        set: set.to_owned(),
        name: name.to_owned(),
        tone: None,
    })
}

/// Parses the `icon = { … }` table form.
pub fn parse_table(table: &toml::Table) -> Result<IconSpec, SpecError> {
    let forms: Vec<&str> = table
        .keys()
        .map(String::as_str)
        .filter(|key| is_form_key(key))
        .collect();
    match forms.as_slice() {
        [] => match table.keys().next() {
            Some(key) => Err(SpecError(format!("unknown icon key `{}`", excerpt(key)))),
            None => Err(SpecError(
                "icon needs one of `set`, `letters`, `glyph` or `svg`".to_owned(),
            )),
        },
        [form] => parse_form(form, table),
        [first, second, ..] => Err(SpecError(format!("icon mixes `{first}` and `{second}`"))),
    }
}

/// Writes the fields an editor shows for a spec.
pub fn to_fields(spec: &IconSpec) -> IconFields {
    match spec {
        IconSpec::Set {
            set,
            name,
            tone: None,
        } if set != "image" && set != "brands" => IconFields::Text(format!("{set}:{name}")),
        IconSpec::Image { file } => IconFields::Text(format!("image:{file}")),
        IconSpec::Brand {
            name,
            colour: BrandColour::Theme,
        } => IconFields::Text(format!("brands:{name}")),
        IconSpec::Set { set, name, tone } => {
            let mut fields = vec![
                ("set", FieldValue::Text(set.clone())),
                ("name", FieldValue::Text(name.clone())),
            ];
            if let Some(tone) = tone {
                fields.push(("tone", FieldValue::Text(tone.name().to_owned())));
            }
            IconFields::Table(fields)
        }
        IconSpec::Brand {
            name,
            colour: BrandColour::Brand,
        } => IconFields::Table(vec![
            ("set", FieldValue::Text("brands".to_owned())),
            ("name", FieldValue::Text(name.clone())),
            ("colour", FieldValue::Text("brand".to_owned())),
        ]),
        IconSpec::Svg { file, mono } => {
            let mut fields = vec![("svg", FieldValue::Text(file.clone()))];
            if !mono {
                fields.push(("mono", FieldValue::Flag(false)));
            }
            IconFields::Table(fields)
        }
        IconSpec::Letters {
            letters,
            shape,
            tone,
        } => {
            let mut fields = vec![("letters", FieldValue::Text(letters.clone()))];
            if *shape != Shape::Soft {
                fields.push(("shape", FieldValue::Text(shape.name().to_owned())));
            }
            if *tone != Tone::Accent {
                fields.push(("tone", FieldValue::Text(tone.name().to_owned())));
            }
            IconFields::Table(fields)
        }
        IconSpec::Glyph { glyph, tone, dot } => {
            let mut fields = vec![("glyph", FieldValue::Text(glyph.clone()))];
            if *tone != Tone::Text {
                fields.push(("tone", FieldValue::Text(tone.name().to_owned())));
            }
            if let Some(dot) = dot {
                fields.push(("dot", FieldValue::Text(dot.name().to_owned())));
            }
            IconFields::Table(fields)
        }
    }
}

fn is_form_key(key: &str) -> bool {
    matches!(key, "set" | "letters" | "glyph" | "svg")
}

fn parse_form(form: &str, table: &toml::Table) -> Result<IconSpec, SpecError> {
    match form {
        "set" => parse_set_form(table),
        "letters" => parse_letters_form(table),
        "glyph" => parse_glyph_form(table),
        "svg" => parse_svg_form(table),
        _ => Err(SpecError(format!("unknown icon key `{form}`"))),
    }
}

fn parse_set_form(table: &toml::Table) -> Result<IconSpec, SpecError> {
    if table.get("set").and_then(toml::Value::as_str) == Some("brands") {
        return parse_brand_form(table);
    }
    allow_keys(table, &["set", "name", "tone"])?;
    let set = text_field(table, "set")?;
    check_set(set)?;
    let name = text_field(table, "name")?;
    check_name(name)?;
    let tone = optional_tone(table, "tone")?;
    Ok(IconSpec::Set {
        set: set.to_owned(),
        name: name.to_owned(),
        tone,
    })
}

fn parse_brand_form(table: &toml::Table) -> Result<IconSpec, SpecError> {
    allow_keys(table, &["set", "name", "colour"])?;
    let name = text_field(table, "name")?;
    check_name(name)?;
    let colour = match optional_text(table, "colour")? {
        None => BrandColour::Theme,
        Some(text) => BrandColour::named(text)
            .ok_or_else(|| SpecError(format!("unknown icon colour `{}`", excerpt(text))))?,
    };
    Ok(IconSpec::Brand {
        name: name.to_owned(),
        colour,
    })
}

fn parse_letters_form(table: &toml::Table) -> Result<IconSpec, SpecError> {
    allow_keys(table, &["letters", "shape", "tone"])?;
    let source = text_field(table, "letters")?;
    let mut chars = source.chars();
    let valid = match (chars.next(), chars.next(), chars.next()) {
        (Some(first), None, None) => first.is_alphanumeric(),
        (Some(first), Some(second), None) => first.is_alphanumeric() && second.is_alphanumeric(),
        _ => false,
    };
    if !valid {
        return Err(invalid_letters(source));
    }
    let letters: String = source.chars().flat_map(char::to_uppercase).collect();
    if letters.chars().count() > 2 {
        return Err(invalid_letters(source));
    }
    let shape = match optional_text(table, "shape")? {
        None => Shape::Soft,
        Some(text) => Shape::named(text)
            .ok_or_else(|| SpecError(format!("unknown icon shape `{}`", excerpt(text))))?,
    };
    let tone = optional_tone(table, "tone")?.unwrap_or(Tone::Accent);
    Ok(IconSpec::Letters {
        letters,
        shape,
        tone,
    })
}

fn parse_glyph_form(table: &toml::Table) -> Result<IconSpec, SpecError> {
    allow_keys(table, &["glyph", "tone", "dot"])?;
    let glyph = text_field(table, "glyph")?;
    check_name(glyph)?;
    let tone = optional_tone(table, "tone")?.unwrap_or(Tone::Text);
    let dot = optional_tone(table, "dot")?;
    Ok(IconSpec::Glyph {
        glyph: glyph.to_owned(),
        tone,
        dot,
    })
}

fn parse_svg_form(table: &toml::Table) -> Result<IconSpec, SpecError> {
    allow_keys(table, &["svg", "mono"])?;
    let file = text_field(table, "svg")?;
    check_file(file, &[".svg"])?;
    let mono = optional_flag(table, "mono")?.unwrap_or(true);
    Ok(IconSpec::Svg {
        file: file.to_owned(),
        mono,
    })
}

fn allow_keys(table: &toml::Table, allowed: &[&str]) -> Result<(), SpecError> {
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(SpecError(format!("unknown icon key `{}`", excerpt(key))));
        }
    }
    Ok(())
}

fn text_field<'a>(table: &'a toml::Table, key: &str) -> Result<&'a str, SpecError> {
    match table.get(key) {
        Some(value) => value
            .as_str()
            .ok_or_else(|| SpecError(format!("icon `{key}` must be text"))),
        None => Err(SpecError(format!("icon is missing `{key}`"))),
    }
}

fn optional_text<'a>(table: &'a toml::Table, key: &str) -> Result<Option<&'a str>, SpecError> {
    match table.get(key) {
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| SpecError(format!("icon `{key}` must be text"))),
        None => Ok(None),
    }
}

fn optional_flag(table: &toml::Table, key: &str) -> Result<Option<bool>, SpecError> {
    match table.get(key) {
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| SpecError(format!("icon `{key}` must be a flag"))),
        None => Ok(None),
    }
}

fn optional_tone(table: &toml::Table, key: &str) -> Result<Option<Tone>, SpecError> {
    match optional_text(table, key)? {
        None => Ok(None),
        Some(text) => Tone::named(text)
            .map(Some)
            .ok_or_else(|| SpecError(format!("unknown icon tone `{}`", excerpt(text)))),
    }
}

fn invalid_letters(source: &str) -> SpecError {
    SpecError(format!(
        "icon letters `{}` must be one or two alphanumeric characters",
        excerpt(source)
    ))
}

fn excerpt(text: &str) -> &str {
    match text.char_indices().nth(64) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

fn check_set(set: &str) -> Result<(), SpecError> {
    if set.len() > 24 {
        return Err(SpecError(format!("invalid icon set `{}`", excerpt(set))));
    }
    let mut chars = set.chars();
    let starts_right = chars.next().is_some_and(|first| first.is_ascii_lowercase());
    let rest_right = chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
    if starts_right && rest_right {
        Ok(())
    } else {
        Err(SpecError(format!("invalid icon set `{}`", excerpt(set))))
    }
}

fn check_name(name: &str) -> Result<(), SpecError> {
    if name.len() > 64 {
        return Err(SpecError(format!("invalid icon name `{}`", excerpt(name))));
    }
    let mut chars = name.chars();
    let starts_right = chars
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit());
    let rest_right =
        chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-');
    if starts_right && rest_right {
        Ok(())
    } else {
        Err(SpecError(format!("invalid icon name `{}`", excerpt(name))))
    }
}

fn check_file(file: &str, extensions: &[&str]) -> Result<(), SpecError> {
    if extensions
        .iter()
        .any(|extension| file_matches(file, extension))
    {
        Ok(())
    } else {
        Err(SpecError(format!("invalid icon file `{}`", excerpt(file))))
    }
}

fn file_matches(file: &str, extension: &str) -> bool {
    if file.len() > 45 {
        return false;
    }
    let Some(stem_and_hash) = file.strip_suffix(extension) else {
        return false;
    };
    let Some((stem, hash)) = stem_and_hash.rsplit_once('-') else {
        return false;
    };
    let stem_ok = !stem.is_empty()
        && stem.len() <= 32
        && stem
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
    let hash_ok = hash.len() == 8 && hash.chars().all(|ch| matches!(ch, '0'..='9' | 'a'..='f'));
    stem_ok && hash_ok
}

#[cfg(test)]
mod tests;
