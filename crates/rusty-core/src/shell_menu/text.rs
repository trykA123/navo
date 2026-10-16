#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanText {
    pub text: String,
    pub access_key: Option<char>,
    pub accelerator: Option<String>,
}

#[must_use]
pub fn clean_text(raw: &str) -> CleanText {
    let (label, accelerator) = match raw.split_once('\t') {
        Some((label, rest)) => (label, Some(rest.trim()).filter(|rest| !rest.is_empty())),
        None => (raw, None),
    };
    let mut text = String::with_capacity(label.len());
    let mut access_key = None;
    let mut chars = label.chars();
    while let Some(c) = chars.next() {
        if c != '&' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('&') => text.push('&'),
            Some(next) => {
                access_key = access_key.or_else(|| next.to_uppercase().next());
                text.push(next);
            }
            None => {}
        }
    }
    CleanText {
        text,
        access_key,
        accelerator: accelerator.map(str::to_owned),
    }
}
