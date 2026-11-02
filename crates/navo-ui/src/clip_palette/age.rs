use navo_l10n::{t, tr};

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const DATE_FROM_DAYS: i64 = 14;

/// How long ago `when_ms` was, as short words; an old time shows as a date.
pub(super) fn age(now_ms: i64, when_ms: i64) -> String {
    let past = now_ms.saturating_sub(when_ms).max(0);
    if past < MINUTE {
        tr("clip-age-now")
    } else if past < HOUR {
        t!("clip-age-minutes", count = past / MINUTE)
    } else if past < DAY {
        t!("clip-age-hours", count = past / HOUR)
    } else if past < 2 * DAY {
        tr("clip-age-yesterday")
    } else if past < DATE_FROM_DAYS * DAY {
        t!("clip-age-days", count = past / DAY)
    } else {
        crate::format::date(when_ms)
    }
}
