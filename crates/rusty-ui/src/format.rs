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

const UNITS: [&str; 6] = ["KB", "MB", "GB", "TB", "PB", "EB"];

pub fn size(count: u64) -> String {
    if count < 1024 {
        return format!("{count} B");
    }
    let mut value = count as f64 / 1024.;
    let mut unit = 0;
    while value >= 1023.95 && unit + 1 < UNITS.len() {
        value /= 1024.;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
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

pub fn clock_in<Zone: TimeZone>(at_ms: i64, zone: &Zone) -> Option<String>
where
    Zone::Offset: std::fmt::Display,
{
    let moment = zone.timestamp_millis_opt(at_ms).single()?;
    Some(moment.format("%H:%M").to_string())
}

pub fn read_time(at_ms: i64) -> String {
    read_time_in(at_ms, Local::now().timestamp_millis(), &Local).unwrap_or_default()
}

pub fn read_time_in<Zone: TimeZone>(at_ms: i64, now_ms: i64, zone: &Zone) -> Option<String>
where
    Zone::Offset: std::fmt::Display,
{
    let day = |ms: i64| {
        zone.timestamp_millis_opt(ms)
            .single()
            .map(|at| at.date_naive())
    };
    if day(at_ms)? == day(now_ms)? {
        clock_in(at_ms, zone)
    } else {
        date_in(at_ms, zone)
    }
}

pub fn count(amount: u64, one: &str, many: &str) -> String {
    let noun = if amount == 1 { one } else { many };
    format!("{} {noun}", bytes(amount))
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, Utc};

    use super::{bytes, clock_in, count, date_in, read_time_in, size};

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
    fn clocks_read_hour_and_minute_in_the_given_zone() {
        let ms = 1_791_920_400_000;
        assert_eq!(clock_in(ms, &Utc).as_deref(), Some("19:40"));
        let bucharest = FixedOffset::east_opt(3 * 3600).expect("valid offset");
        assert_eq!(clock_in(ms, &bucharest).as_deref(), Some("22:40"));
    }

    #[test]
    fn read_times_show_the_date_only_when_not_from_today() {
        let ms = 1_791_920_400_000;
        let bucharest = FixedOffset::east_opt(3 * 3600).expect("valid offset");
        let later_that_day = ms + 3_600_000;
        let next_day = ms + 2 * 3_600_000;
        assert_eq!(
            read_time_in(ms, later_that_day, &bucharest).as_deref(),
            Some("22:40")
        );
        assert_eq!(
            read_time_in(ms, next_day, &bucharest).as_deref(),
            Some("2026-10-13 22:40")
        );
        assert_eq!(read_time_in(ms, next_day, &Utc).as_deref(), Some("19:40"));
    }

    #[test]
    fn counts_use_the_singular_for_one() {
        assert_eq!(count(1, "file", "files"), "1 file");
        assert_eq!(count(0, "file", "files"), "0 files");
        assert_eq!(count(1_200, "folder", "folders"), "1,200 folders");
    }

    #[test]
    fn sizes_read_in_binary_units_with_one_decimal() {
        assert_eq!(size(0), "0 B");
        assert_eq!(size(1_023), "1023 B");
        assert_eq!(size(1_024), "1.0 KB");
        assert_eq!(size(1_048_575), "1.0 MB");
        assert_eq!(size(1_288_490_189), "1.2 GB");
        assert_eq!(size(1_313_828_071_552), "1.2 TB");
        assert_eq!(size(3 << 50), "3.0 PB");
        assert_eq!(size(u64::MAX), "16.0 EB");
    }
}
