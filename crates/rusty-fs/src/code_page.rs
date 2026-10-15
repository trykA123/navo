use encoding_rs::{BIG5, EUC_KR, Encoding, GBK, SHIFT_JIS, UTF_8, WINDOWS_1252};

#[must_use]
pub fn ansi_code_page() -> &'static Encoding {
    WINDOWS_1252
}

#[must_use]
pub fn encoding_for_code_page(code_page: u32) -> &'static Encoding {
    match code_page {
        932 => SHIFT_JIS,
        936 => GBK,
        949 => EUC_KR,
        950 => BIG5,
        65001 => UTF_8,
        874 | 1250..=1258 => {
            Encoding::for_label(format!("windows-{code_page}").as_bytes()).unwrap_or(WINDOWS_1252)
        }
        _ => WINDOWS_1252,
    }
}

#[cfg(test)]
mod tests {
    use encoding_rs::{WINDOWS_874, WINDOWS_1250, WINDOWS_1251};

    use super::*;

    #[test]
    fn windows_code_pages_map_to_their_encodings() {
        let cases = [
            (1250, WINDOWS_1250),
            (1251, WINDOWS_1251),
            (1252, WINDOWS_1252),
            (874, WINDOWS_874),
            (932, SHIFT_JIS),
            (936, GBK),
            (949, EUC_KR),
            (950, BIG5),
            (65001, UTF_8),
            (437, WINDOWS_1252),
        ];
        for (code_page, encoding) in cases {
            assert_eq!(encoding_for_code_page(code_page), encoding, "{code_page}");
        }
    }
}
