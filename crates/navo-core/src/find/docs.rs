//! Text from document XML parts (FR-D11): a linear byte scanner with no DTD, only the five named
//! entities and numeric entities, and a bound on output size. No allocation per tag.

/// Why an XML part cannot be read as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DocsError {
    /// The part declares a DTD.
    #[error("the part declares a DTD")]
    Dtd,
    /// The part is not UTF-8.
    #[error("the part is not UTF-8")]
    Encoding,
    /// The text is longer than the limit.
    #[error("the text is too large")]
    TooLarge,
}

/// Paragraph-like closing tags that end a line of text.
const LINE_ENDS: &[&str] = &[
    "w:p", "a:p", "text:p", "text:h", "p", "h1", "h2", "h3", "h4", "h5", "h6", "li", "si",
];
/// Self-closing tab and space elements that stand for one space.
const SPACES: &[&str] = &["w:tab", "text:tab", "text:s"];
/// Longest entity looked at, `&` to `;` included (`&#x10FFFF;`, `&#1114111;`).
const ENTITY_MAX: usize = 11;

/// Appends the text of the XML part `xml` to `out`, at most `limit` bytes.
///
/// Text between tags is kept, CDATA verbatim, tab and space elements as one space; comments and
/// processing instructions are skipped.
/// Any `<!` other than a comment or CDATA (a `<!DOCTYPE`) fails with [`DocsError::Dtd`]. On an
/// error `out` may hold part of the text. Unterminated markup ends the text quietly.
pub fn xml_text(xml: &[u8], out: &mut String, limit: usize) -> Result<(), DocsError> {
    if xml.starts_with(b"\xFF\xFE") || xml.starts_with(b"\xFE\xFF") {
        return Err(DocsError::Encoding);
    }
    let xml = xml.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(xml);
    let mut rest = std::str::from_utf8(xml).map_err(|_| DocsError::Encoding)?;
    let end = out.len().saturating_add(limit);
    let mut sink = Sink { out, end };
    while let Some(open) = rest.find(['<', '&']) {
        sink.push(&rest[..open])?;
        let next = if rest[open..].starts_with('&') {
            Some(entity(&rest[open..], &mut sink)?)
        } else {
            markup(&rest[open..], &mut sink)?
        };
        match next {
            Some(next) => rest = next,
            None => return Ok(()),
        }
    }
    sink.push(rest)
}

/// Bounded writer over the caller's string.
struct Sink<'a> {
    out: &'a mut String,
    end: usize,
}

impl Sink<'_> {
    fn push(&mut self, text: &str) -> Result<(), DocsError> {
        if self.out.len().saturating_add(text.len()) > self.end {
            return Err(DocsError::TooLarge);
        }
        self.out.push_str(text);
        Ok(())
    }

    fn push_char(&mut self, character: char) -> Result<(), DocsError> {
        self.push(character.encode_utf8(&mut [0; 4]))
    }
}

/// Decodes the entity at the start of `rest` (which starts with `&`); an unknown one stays.
fn entity<'a>(rest: &'a str, sink: &mut Sink<'_>) -> Result<&'a str, DocsError> {
    let semicolon = rest.bytes().take(ENTITY_MAX).position(|byte| byte == b';');
    if let Some(semicolon) = semicolon
        && let Some(character) = decode(&rest[1..semicolon])
    {
        sink.push_char(character)?;
        return Ok(&rest[semicolon + 1..]);
    }
    sink.push("&")?;
    Ok(&rest[1..])
}

fn decode(name: &str) -> Option<char> {
    let (digits, radix) = match name {
        "lt" => return Some('<'),
        "gt" => return Some('>'),
        "amp" => return Some('&'),
        "quot" => return Some('"'),
        "apos" => return Some('\''),
        _ => match name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
            Some(hex) => (hex, 16),
            None => (name.strip_prefix('#')?, 10),
        },
    };
    if digits.is_empty() || !digits.chars().all(|digit| digit.is_digit(radix)) {
        return None;
    }
    char::from_u32(u32::from_str_radix(digits, radix).ok()?)
}

/// Handles the markup at the start of `rest` (which starts with `<`); `None` when unterminated.
fn markup<'a>(rest: &'a str, sink: &mut Sink<'_>) -> Result<Option<&'a str>, DocsError> {
    if let Some(body) = rest.strip_prefix("<!--") {
        return Ok(after(body, "-->"));
    }
    if let Some(body) = rest.strip_prefix("<![CDATA[") {
        let Some(close) = body.find("]]>") else {
            return Ok(None);
        };
        sink.push(&body[..close])?;
        return Ok(Some(&body[close + 3..]));
    }
    if rest.starts_with("<!") {
        return Err(DocsError::Dtd);
    }
    if let Some(body) = rest.strip_prefix("<?") {
        return Ok(after(body, "?>"));
    }
    let Some(close) = tag_end(rest) else {
        return Ok(None);
    };
    let tag = &rest[1..close];
    let closing = tag.starts_with('/');
    let name = tag
        .trim_start_matches('/')
        .split(|c: char| c.is_ascii_whitespace() || c == '/')
        .next()
        .unwrap_or_default();
    let breaks = if closing {
        LINE_ENDS.contains(&name)
    } else {
        name == "br" || name == "w:br"
    };
    if breaks {
        sink.push("\n")?;
    } else if tag.ends_with('/') && SPACES.contains(&name) {
        sink.push(" ")?;
    }
    Ok(Some(&rest[close + 1..]))
}

fn after<'a>(body: &'a str, close: &str) -> Option<&'a str> {
    body.find(close).map(|at| &body[at + close.len()..])
}

/// Index of the `>` that ends the tag at the start of `rest`, skipping quoted attribute values.
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote = None;
    for (index, byte) in rest.bytes().enumerate() {
        match (quote, byte) {
            (None, b'"' | b'\'') => quote = Some(byte),
            (Some(open), _) if open == byte => quote = None,
            (None, b'>') => return Some(index),
            _ => {}
        }
    }
    None
}
