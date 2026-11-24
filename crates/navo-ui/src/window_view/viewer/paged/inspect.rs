//! The hex inspector's decode table: what the bytes at the hex cursor mean as numbers, chars,
//! times and bits. Pure; the drawer in `inspector.rs` shows it.

/// The bytes the widest row needs (i64, u64, f64, a UTF-16 surrogate pair).
pub(in crate::window_view::viewer) const INSPECT_BYTES: usize = 8;

const DASH: &str = "—";
const SECONDS_PER_DAY: i64 = 86_400;
/// Days from 0000-03-01 to 1970-01-01, the shift of the civil-from-days algorithm below.
const EPOCH_SHIFT: i64 = 719_468;
const LAST_YEAR: i64 = 9999;

/// One decoded row: its Fluent label id and the shown value ("—" when it cannot be decoded).
pub(in crate::window_view::viewer) struct InspectRow {
    pub id: &'static str,
    pub value: String,
}

/// Every decoded row for the bytes at the cursor, in drawer order.
pub(in crate::window_view::viewer) struct Inspection {
    pub rows: Vec<InspectRow>,
}

impl Inspection {
    fn push(&mut self, id: &'static str, value: Option<String>) {
        let value = value.unwrap_or_else(|| DASH.to_owned());
        self.rows.push(InspectRow { id, value });
    }
}

/// The first `N` bytes as an array, or None when fewer are left.
fn take<const N: usize>(bytes: &[u8]) -> Option<[u8; N]> {
    bytes.get(..N)?.try_into().ok()
}

/// Pushes the little- and big-endian reading of each listed number type, shown with Rust's
/// `{:?}` (floats keep their ".0").
macro_rules! both_ends {
    ($out:ident, $bytes:ident; $($ty:ty => $le:literal, $be:literal;)+) => {$({
        let raw = take::<{ size_of::<$ty>() }>($bytes);
        $out.push($le, raw.map(|raw| format!("{:?}", <$ty>::from_le_bytes(raw))));
        $out.push($be, raw.map(|raw| format!("{:?}", <$ty>::from_be_bytes(raw))));
    })+};
}

/// A char as the glyph and its code point; control chars show the code point only.
fn show_char(found: char) -> String {
    let code = format!("U+{:04X}", u32::from(found));
    if found.is_control() {
        code
    } else {
        format!("{found} {code}")
    }
}

/// The UTF-8 char that starts at the first byte, or None when it is not a valid lead byte.
fn utf8_char(bytes: &[u8]) -> Option<String> {
    let len = match bytes.first()? {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return None,
    };
    let text = std::str::from_utf8(bytes.get(..len)?).ok()?;
    text.chars().next().map(show_char)
}

/// The UTF-16 char at the first unit, reading a surrogate pair when the first unit is high.
fn utf16_char(bytes: &[u8], read: fn([u8; 2]) -> u16) -> Option<String> {
    let first = read(take(bytes)?);
    let mut units = vec![first];
    if (0xD800..0xDC00).contains(&first) {
        units.push(read(take(bytes.get(2..)?)?));
    }
    char::decode_utf16(units).next()?.ok().map(show_char)
}

/// The civil date (year, month, day) of a day count from 1970-01-01, after Howard Hinnant's
/// days-to-civil algorithm (400-year eras of 146 097 days).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + EPOCH_SHIFT;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Seconds since 1970-01-01 UTC as ISO-8601 ("1970-01-01T00:00:00Z"); None outside years
/// 0000 to 9999.
fn utc(seconds: i64) -> Option<String> {
    let (year, month, day) = civil_from_days(seconds.div_euclid(SECONDS_PER_DAY));
    if !(0..=LAST_YEAR).contains(&year) {
        return None;
    }
    let time = seconds.rem_euclid(SECONDS_PER_DAY);
    let (hour, minute, second) = (time / 3600, time / 60 % 60, time % 60);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

/// The integer and float rows: i8 and u8, then every wider type in both byte orders.
fn number_rows(bytes: &[u8], out: &mut Inspection) {
    let first = bytes.first().copied();
    out.push(
        "hex-inspector-i8",
        first.map(|byte| (byte as i8).to_string()),
    );
    out.push("hex-inspector-u8", first.map(|byte| byte.to_string()));
    both_ends!(out, bytes;
        i16 => "hex-inspector-i16-le", "hex-inspector-i16-be";
        u16 => "hex-inspector-u16-le", "hex-inspector-u16-be";
        i32 => "hex-inspector-i32-le", "hex-inspector-i32-be";
        u32 => "hex-inspector-u32-le", "hex-inspector-u32-be";
        i64 => "hex-inspector-i64-le", "hex-inspector-i64-be";
        u64 => "hex-inspector-u64-le", "hex-inspector-u64-be";
        f32 => "hex-inspector-f32-le", "hex-inspector-f32-be";
        f64 => "hex-inspector-f64-le", "hex-inspector-f64-be";
    );
}

/// The char, time and bit rows: UTF-8 and UTF-16 chars, Unix times (LE) and the first byte's bits.
fn text_rows(bytes: &[u8], out: &mut Inspection) {
    out.push("hex-inspector-utf8", utf8_char(bytes));
    let utf16_le = utf16_char(bytes, u16::from_le_bytes);
    out.push("hex-inspector-utf16-le", utf16_le);
    let utf16_be = utf16_char(bytes, u16::from_be_bytes);
    out.push("hex-inspector-utf16-be", utf16_be);
    let unix32 = take(bytes).map(|raw| i64::from(u32::from_le_bytes(raw)));
    out.push("hex-inspector-unix-u32", unix32.and_then(utc));
    let unix64 = take(bytes).map(i64::from_le_bytes);
    out.push("hex-inspector-unix-i64", unix64.and_then(utc));
    let bits = bytes.first().map(|byte| format!("{byte:08b}"));
    out.push("hex-inspector-bits", bits);
}

/// Decodes `bytes` (those from the cursor on, up to 8) into the inspector rows: integers and
/// floats in both byte orders, UTF-8 and UTF-16 chars, Unix times (LE) and the first byte's bits.
pub(in crate::window_view::viewer) fn inspect(bytes: &[u8]) -> Inspection {
    let mut out = Inspection { rows: Vec::new() };
    number_rows(bytes, &mut out);
    text_rows(bytes, &mut out);
    out
}
