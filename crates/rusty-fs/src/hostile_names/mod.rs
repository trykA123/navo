use rusty_core::ProviderPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Unix,
    Windows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostileName {
    pub label: &'static str,
    pub name: ProviderPath,
    pub unix: bool,
    pub windows: bool,
}

const LONG_SEGMENTS: usize = 6;
const LONG_SEGMENT_LEN: usize = 60;
const MAX_NAME: usize = 255;

fn named(label: &'static str, name: ProviderPath, unix: bool, windows: bool) -> HostileName {
    HostileName {
        label,
        name,
        unix,
        windows,
    }
}

fn both(label: &'static str, text: &str) -> HostileName {
    named(label, ProviderPath::from_text(text), true, true)
}

fn literal(label: &'static str, text: &str) -> HostileName {
    named(label, ProviderPath::from_bytes(text.as_bytes()), true, true)
}

fn unix_text(label: &'static str, text: &str) -> HostileName {
    named(label, ProviderPath::from_text(text), true, false)
}

fn unix_bytes(label: &'static str, bytes: &[u8]) -> HostileName {
    named(label, ProviderPath::from_bytes(bytes), true, false)
}

fn windows_wide(label: &'static str, units: &[u16]) -> HostileName {
    named(label, ProviderPath::from_wide(units), false, true)
}

fn length() -> Vec<HostileName> {
    vec![
        both("ascii_255_chars", &"x".repeat(MAX_NAME)),
        named(
            "e_acute_255_utf16_units",
            ProviderPath::from_text(&"\u{e9}".repeat(MAX_NAME)),
            false,
            true,
        ),
    ]
}

fn invalid_utf16() -> Vec<HostileName> {
    vec![
        windows_wide("lone_high_surrogate", &[0xD800]),
        windows_wide("lone_low_surrogate", &[0xDC00]),
        windows_wide("reversed_surrogate_pair", &[0xDC00, 0xD800]),
        windows_wide("surrogate_between_letters", &[0x61, 0xD83D, 0x62]),
    ]
}

fn invalid_utf8() -> Vec<HostileName> {
    vec![
        unix_bytes("byte_ff", &[0xFF]),
        unix_bytes("overlong_slash", &[0xC0, 0xAF]),
        unix_bytes("truncated_three_byte", &[b'a', 0xE2, 0x82]),
        unix_bytes("latin1_cafe", &[b'c', b'a', b'f', 0xE9]),
    ]
}

fn reserved_dos() -> Vec<HostileName> {
    vec![
        both("dos_con", "CON"),
        both("dos_nul_lowercase", "nul"),
        both("dos_aux_with_extension", "AUX.txt"),
        both("dos_com1", "COM1"),
        both("dos_lpt9_with_extension", "LPT9.log"),
        both("dos_com_superscript_one", "COM\u{b9}"),
    ]
}

fn edges() -> Vec<HostileName> {
    vec![
        both("trailing_dot", "name."),
        both("trailing_space", "name "),
        both("trailing_dot_space_dot", "name. ."),
        both("leading_space", " name"),
        both("leading_dot", ".hidden"),
        both("leading_dash", "-rf"),
        both("three_dots", "..."),
    ]
}

fn windows_illegal() -> Vec<HostileName> {
    vec![
        unix_text("colon", "a:b"),
        unix_text("backslash", "a\\b"),
        unix_text("asterisk", "a*b"),
        unix_text("question_mark", "a?b"),
        unix_text("angle_brackets", "a<b>c"),
        unix_text("pipe", "a|b"),
        unix_text("double_quote", "a\"b"),
        unix_text("control_char", "a\u{1}b"),
        unix_text("newline", "a\nb"),
    ]
}

fn case() -> Vec<HostileName> {
    vec![
        both("mixed_case", "Readme.md"),
        unix_text("upper_case_twin", "README.md"),
    ]
}

fn unicode() -> Vec<HostileName> {
    vec![
        both("nfc_e_acute", "\u{e9}t\u{e9}"),
        both("nfd_e_acute", "e\u{301}te\u{301}"),
        both("emoji_zwj", "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}"),
        both("right_to_left_override", "report\u{202e}fdp.exe"),
        both("zero_width_space", "a\u{200b}b"),
        both("cjk", "\u{6587}\u{4ef6}"),
        both("greek_final_sigma", "\u{3bb}\u{3cc}\u{3b3}\u{3bf}\u{3c2}"),
        both("turkish_dotless_i", "d\u{131}r"),
        both("full_width_letters", "\u{ff21}\u{ff22}\u{ff23}"),
        both("sharp_s", "stra\u{df}e"),
    ]
}

fn escape_collisions() -> Vec<HostileName> {
    vec![
        literal("wide_escape_alone", "\u{100000}"),
        literal("byte_escape_alone", "\u{1008ff}"),
        literal("wide_escape_between_letters", "a\u{1003ff}b"),
        literal("byte_escape_between_letters", "a\u{100880}b"),
        literal("escape_gap_char", "\u{100800}"),
        literal("one_of_each_escape_range", "\u{1007ff}\u{100800}\u{100880}"),
    ]
}

fn display_separators() -> Vec<HostileName> {
    vec![
        both("percent_encoded_slash", "a%2Fb"),
        both("division_slash", "a\u{2215}b"),
    ]
}

#[must_use]
pub fn corpus() -> Vec<HostileName> {
    [
        length(),
        invalid_utf16(),
        invalid_utf8(),
        reserved_dos(),
        edges(),
        windows_illegal(),
        case(),
        unicode(),
        escape_collisions(),
        display_separators(),
    ]
    .concat()
}

#[must_use]
pub fn for_family(family: Family) -> Vec<HostileName> {
    corpus()
        .into_iter()
        .filter(|n| match family {
            Family::Unix => n.unix,
            Family::Windows => n.windows,
        })
        .collect()
}

#[must_use]
pub fn long_path_segments() -> Vec<ProviderPath> {
    ('a'..='z')
        .take(LONG_SEGMENTS)
        .map(|c| ProviderPath::from_text(&c.to_string().repeat(LONG_SEGMENT_LEN)))
        .collect()
}

#[cfg(test)]
mod tests;
