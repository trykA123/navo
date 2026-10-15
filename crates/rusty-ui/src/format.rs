use chrono::{Local, TimeZone};

pub fn bytes(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

pub fn date(modified_ms: i64) -> String {
    date_in(modified_ms, &Local).unwrap_or_default()
}

pub fn date_in<Zone: TimeZone>(modified_ms: i64, zone: &Zone) -> Option<String>
where
    Zone::Offset: std::fmt::Display,
{
    let moment = zone.timestamp_millis_opt(modified_ms).single()?;
    Some(moment.format("%Y-%m-%d %H:%M").to_string())
}

pub fn count(amount: u64, one: &str, many: &str) -> String {
    let noun = if amount == 1 { one } else { many };
    format!("{} {noun}", bytes(amount))
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, Utc};

    use super::{bytes, count, date_in};

    #[test]
    fn bytes_group_thousands_with_commas() {
        assert_eq!(bytes(0), "0");
        assert_eq!(bytes(812), "812");
        assert_eq!(bytes(1_000), "1,000");
        assert_eq!(bytes(37_412), "37,412");
        assert_eq!(bytes(1_234_567_890), "1,234,567,890");
    }

    #[test]
    fn dates_read_year_month_day_hour_minute_in_the_given_zone() {
        let ms = 1_791_920_400_000;
        assert_eq!(date_in(ms, &Utc).as_deref(), Some("2026-10-13 19:40"));
        let bucharest = FixedOffset::east_opt(3 * 3600).expect("valid offset");
        assert_eq!(date_in(ms, &bucharest).as_deref(), Some("2026-10-13 22:40"));
    }

    #[test]
    fn counts_use_the_singular_for_one() {
        assert_eq!(count(1, "file", "files"), "1 file");
        assert_eq!(count(0, "file", "files"), "0 files");
        assert_eq!(count(1_200, "folder", "folders"), "1,200 folders");
    }
}
