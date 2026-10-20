#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    All,
    At(i32),
    Range(i32, i32),
    Len(i32, u32),
    From(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterSpec {
    pub start: u64,
    pub step: u64,
    pub digits: u8,
}

impl Default for CounterSpec {
    fn default() -> Self {
        Self {
            start: 1,
            step: 1,
            digits: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Text(String),
    Name(Span),
    Ext(Span),
    Parent,
    Grand,
    Counter(Option<CounterSpec>),
    Year,
    ShortYear,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    Date,
    Time,
    Case(CaseMode),
    Field { group: String, field: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseMode {
    Original,
    Upper,
    Lower,
    Words,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    pub(super) tokens: Vec<Token>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MaskErrorKind {
    #[error("unknown token")]
    UnknownToken,
    #[error("missing ]")]
    Unclosed,
    #[error("] without [")]
    StrayClose,
    #[error("invalid name or extension range")]
    BadSpan,
    #[error("invalid counter")]
    BadCounter,
    #[error("invalid field")]
    BadField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at character {at}")]
pub struct MaskError {
    pub at: usize,
    pub kind: MaskErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

pub type FieldLookup<'a> = &'a dyn Fn(&str, &str) -> Option<String>;

#[derive(Clone, Copy)]
pub struct Subject<'a> {
    pub stem: &'a str,
    pub ext: &'a str,
    pub parent: &'a str,
    pub grand: &'a str,
    pub time: Option<LocalTime>,
    pub fields: Option<FieldLookup<'a>>,
}

impl Mask {
    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    #[must_use]
    pub fn render(&self, subject: &Subject<'_>, counter: u64) -> String {
        self.render_with(subject, counter, CounterSpec::default())
    }

    #[must_use]
    pub fn render_with(
        &self,
        subject: &Subject<'_>,
        counter: u64,
        default_counter: CounterSpec,
    ) -> String {
        let mut out = String::new();
        let mut mode = CaseMode::Original;
        for token in &self.tokens {
            let text = match token {
                Token::Text(text) => text.clone(),
                Token::Name(span) => slice(subject.stem, *span),
                Token::Ext(span) => slice(subject.ext, *span),
                Token::Parent => subject.parent.to_owned(),
                Token::Grand => subject.grand.to_owned(),
                Token::Counter(spec) => number(spec.unwrap_or(default_counter), counter),
                Token::Case(next) => {
                    mode = *next;
                    continue;
                }
                Token::Field { group, field } => subject
                    .fields
                    .and_then(|lookup| lookup(group, field))
                    .unwrap_or_default(),
                date => date_part(date, subject.time),
            };
            push_cased(&mut out, &text, mode);
        }
        out
    }
}

fn push_cased(out: &mut String, text: &str, mode: CaseMode) {
    match mode {
        CaseMode::Original => out.push_str(text),
        CaseMode::Upper => out.extend(text.chars().flat_map(char::to_uppercase)),
        CaseMode::Lower => out.extend(text.chars().flat_map(char::to_lowercase)),
        CaseMode::Words => {
            for ch in text.chars() {
                let starts_word = out
                    .chars()
                    .next_back()
                    .is_none_or(|prev| matches!(prev, ' ' | '_' | '-' | '.'));
                if starts_word {
                    out.extend(ch.to_uppercase());
                } else {
                    out.push(ch);
                }
            }
        }
    }
}

fn number(spec: CounterSpec, ordinal: u64) -> String {
    let value = spec.start.saturating_add(spec.step.saturating_mul(ordinal));
    format!("{value:0>width$}", width = usize::from(spec.digits))
}

fn date_part(token: &Token, time: Option<LocalTime>) -> String {
    let Some(t) = time else {
        return String::new();
    };
    match token {
        Token::Year => format!("{:04}", t.year),
        Token::ShortYear => format!("{:02}", t.year % 100),
        Token::Month => format!("{:02}", t.month),
        Token::Day => format!("{:02}", t.day),
        Token::Hour => format!("{:02}", t.hour),
        Token::Minute => format!("{:02}", t.minute),
        Token::Second => format!("{:02}", t.second),
        Token::Date => format!("{:04}-{:02}-{:02}", t.year, t.month, t.day),
        _ => format!("{:02}.{:02}.{:02}", t.hour, t.minute, t.second),
    }
}

fn slice(text: &str, span: Span) -> String {
    if span == Span::All {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let range = match span {
        Span::All => None,
        Span::At(pos) => start_index(pos, len).map(|s| s..s + 1),
        Span::From(pos) => start_index(pos, len).map(|s| s..len),
        Span::Len(pos, count) => start_index(pos, len).map(|s| {
            let count = usize::try_from(count).unwrap_or(usize::MAX);
            s..s.saturating_add(count).min(len)
        }),
        Span::Range(from, to) => start_index(from, len).and_then(|s| {
            let e = end_index(to, len)?;
            (e >= s).then_some(s..e + 1)
        }),
    };
    range.map_or_else(String::new, |r| chars[r].iter().collect())
}

fn start_index(pos: i32, len: usize) -> Option<usize> {
    resolve(pos, len).filter(|&index| index < len)
}

fn end_index(pos: i32, len: usize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    resolve(pos, len).map(|index| index.min(last))
}

fn resolve(pos: i32, len: usize) -> Option<usize> {
    let len = i64::try_from(len).ok()?;
    let index = match pos {
        0 => return None,
        1.. => i64::from(pos) - 1,
        _ => len + i64::from(pos),
    };
    usize::try_from(index).ok()
}
