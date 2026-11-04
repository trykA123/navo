/// An HTML page under construction. Markup is fixed text; everything else is escaped.
/// The text inputs are kept as given so the writer can scan them before escaping.
#[derive(Debug, Default)]
pub struct Html {
    buf: String,
    raw: Vec<String>,
}

impl Html {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn markup(&mut self, fixed: &'static str) -> &mut Self {
        self.buf.push_str(fixed);
        self
    }

    pub fn text(&mut self, text: &str) -> &mut Self {
        self.raw.push(text.to_owned());
        for ch in text.chars() {
            match ch {
                '&' => self.buf.push_str("&amp;"),
                '<' => self.buf.push_str("&lt;"),
                '>' => self.buf.push_str("&gt;"),
                '"' => self.buf.push_str("&quot;"),
                '\'' => self.buf.push_str("&#39;"),
                _ => self.buf.push(ch),
            }
        }
        self
    }

    pub fn uint(&mut self, value: u64) -> &mut Self {
        self.buf.push_str(&value.to_string());
        self
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.buf.as_bytes()
    }

    /// Every `text` input as given, before escaping.
    pub(super) fn raw_texts(&self) -> impl Iterator<Item = &[u8]> {
        self.raw.iter().map(String::as_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::Html;

    #[test]
    fn text_is_escaped_and_markup_is_not() {
        let mut html = Html::new();
        html.markup("<p>").text("a<b>&\"'").uint(7).markup("</p>");
        let page = String::from_utf8_lossy(html.as_bytes()).into_owned();
        assert_eq!(page, "<p>a&lt;b&gt;&amp;&quot;&#39;7</p>");
    }
}
