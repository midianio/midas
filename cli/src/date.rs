//! Calendar dates as `YYYY-MM-DD`, without a date crate.
//!
//! Howard Hinnant's public-domain civil-date algorithms. Used by `midas sync` (today's
//! `last_reviewed`) and by source-drift (how many days a change has been sitting).

use std::time::{SystemTime, UNIX_EPOCH};

/// Today's UTC date as `YYYY-MM-DD`.
pub fn today_ymd() -> String {
    format_ymd(civil_from_days(unix_epoch_days()))
}

/// Days between two `YYYY-MM-DD` strings (`b - a`), or `None` if either is not a date.
pub fn days_between(a: &str, b: &str) -> Option<i64> {
    Some(ymd_to_days(b)? - ymd_to_days(a)?)
}

/// Parse `YYYY-MM-DD` into days since the Unix epoch.
pub fn ymd_to_days(s: &str) -> Option<i64> {
    if !is_iso_date(s) {
        return None;
    }
    let y: i64 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| days_from_civil(y, m, d))
}

/// Whether `s` is a `YYYY-MM-DD` digit string. Does not validate the calendar (2026-13-40 passes).
pub fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn unix_epoch_days() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86400)
        .unwrap_or(0) as i64
}

fn format_ymd((y, m, d): (i64, u32, u32)) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since the Unix epoch → a proleptic-Gregorian `(year, month, day)`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d)
}

/// A proleptic-Gregorian `(year, month, day)` → days since the Unix epoch.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u64 + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

/// Calendar dates are **UTC**. A date-only `YYYY-MM-DD` is that UTC day; a timestamp is converted
/// to its UTC calendar date before comparison so `2026-08-24T22:00:00Z` and `2026-08-24` agree.
pub fn utc_calendar_date(s: &str) -> Option<String> {
    let s = s.trim().trim_matches(['"', '\'']);
    if is_iso_date(s) {
        return Some(s.to_string());
    }
    if s.len() < 19 || !is_iso_date(&s[..10]) {
        return None;
    }
    let sep = s.as_bytes()[10];
    if sep != b'T' && sep != b' ' {
        return None;
    }
    let y: i64 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    let h: i64 = s[11..13].parse().ok()?;
    let min: i64 = s[14..16].parse().ok()?;
    let sec: i64 = s[17..19].parse().ok()?;
    if !(0..24).contains(&h) || !(0..60).contains(&min) || !(0..61).contains(&sec) {
        return None;
    }
    let offset_min = parse_utc_offset(&s[19..])?;
    let mut days = days_from_civil(y, m, d);
    let mut minutes = h * 60 + min - offset_min;
    while minutes < 0 {
        minutes += 24 * 60;
        days -= 1;
    }
    while minutes >= 24 * 60 {
        minutes -= 24 * 60;
        days += 1;
    }
    Some(format_ymd(civil_from_days(days)))
}

/// Parse `Z`, `+HH:MM`, `+HHMM`, or `+HH` after an optional fractional-second suffix.
fn parse_utc_offset(rest: &str) -> Option<i64> {
    let rest = rest.trim();
    let rest = if let Some(stripped) = rest.strip_prefix('.') {
        let digits = stripped.bytes().take_while(|b| b.is_ascii_digit()).count();
        stripped[digits..].trim()
    } else {
        rest
    };
    if rest.is_empty() || rest.eq_ignore_ascii_case("Z") {
        return Some(0);
    }
    let (sign, rest) = match rest.as_bytes().first()? {
        b'+' => (1i64, &rest[1..]),
        b'-' => (-1i64, &rest[1..]),
        _ => return None,
    };
    let rest = rest.trim();
    let (hh, mm) = if rest.len() >= 5 && rest.as_bytes()[2] == b':' {
        (
            rest[0..2].parse::<i64>().ok()?,
            rest[3..5].parse::<i64>().ok()?,
        )
    } else if rest.len() >= 4 {
        (
            rest[0..2].parse::<i64>().ok()?,
            rest[2..4].parse::<i64>().ok()?,
        )
    } else if rest.len() >= 2 {
        (rest[0..2].parse::<i64>().ok()?, 0)
    } else {
        return None;
    };
    Some(sign * (hh * 60 + mm))
}

/// A source change after `last_reviewed` is due for enforcement once `grace_days` have elapsed
/// since the change (UTC today). `grace_days == 0` means fail as soon as the dates disagree —
/// `DOC-0004`'s contract. Clock-skew that puts `changed` in the future is treated as not-yet-due
/// when a grace window is in play, so a bad clock cannot invent a failure.
///
/// Inputs are normalized to UTC calendar dates first: a timestamp and a date-only value on the
/// same UTC day do not fail.
pub fn drift_is_due(changed: &str, reviewed: &str, today: &str, grace_days: u32) -> bool {
    let changed = utc_calendar_date(changed).unwrap_or_else(|| changed.to_string());
    let reviewed = utc_calendar_date(reviewed).unwrap_or_else(|| reviewed.to_string());
    let today = utc_calendar_date(today).unwrap_or_else(|| today.to_string());
    let Some(changed_days) = ymd_to_days(&changed) else {
        return changed > reviewed;
    };
    let Some(reviewed_days) = ymd_to_days(&reviewed) else {
        return changed > reviewed;
    };
    if changed_days <= reviewed_days {
        return false;
    }
    if grace_days == 0 {
        return true;
    }
    match days_between(&changed, &today) {
        Some(elapsed) => elapsed >= i64::from(grace_days),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_epoch_is_day_zero() {
        assert_eq!(ymd_to_days("1970-01-01"), Some(0));
        assert_eq!(format_ymd(civil_from_days(0)), "1970-01-01");
    }

    #[test]
    fn known_span() {
        assert_eq!(days_between("2026-08-11", "2026-08-18"), Some(7));
        assert_eq!(days_between("2026-08-18", "2026-08-11"), Some(-7));
    }

    #[test]
    fn roundtrip_nearby_days() {
        for z in -40..=40 {
            let ymd = format_ymd(civil_from_days(z));
            assert_eq!(ymd_to_days(&ymd), Some(z), "{ymd}");
        }
    }

    #[test]
    fn grace_zero_fires_the_day_the_source_moves() {
        assert!(drift_is_due("2026-08-18", "2026-08-01", "2026-08-18", 0));
        assert!(!drift_is_due("2026-08-01", "2026-08-01", "2026-08-18", 0));
        assert!(!drift_is_due("2026-07-31", "2026-08-01", "2026-08-18", 0));
    }

    #[test]
    fn grace_seven_waits_a_week_after_the_change() {
        assert!(!drift_is_due("2026-08-18", "2026-08-01", "2026-08-18", 7));
        assert!(!drift_is_due("2026-08-11", "2026-08-01", "2026-08-17", 7));
        assert!(drift_is_due("2026-08-11", "2026-08-01", "2026-08-18", 7));
        assert!(!drift_is_due("2026-08-11", "2026-08-12", "2026-08-18", 7));
    }

    #[test]
    fn timestamp_and_date_only_on_the_same_utc_day_do_not_drift() {
        assert!(!drift_is_due(
            "2026-08-24T22:00:00Z",
            "2026-08-24",
            "2026-08-24",
            0
        ));
        assert!(!drift_is_due(
            "2026-08-24T23:30:00+00:00",
            "2026-08-24",
            "2026-08-24",
            0
        ));
    }

    #[test]
    fn west_of_utc_evening_crosses_into_the_next_utc_day() {
        // 23:00-07 on Aug 24 is 06:00 UTC on Aug 25.
        assert_eq!(
            utc_calendar_date("2026-08-24T23:00:00-07:00").as_deref(),
            Some("2026-08-25")
        );
        assert!(drift_is_due(
            "2026-08-24T23:00:00-07:00",
            "2026-08-24",
            "2026-08-25",
            0
        ));
    }

    #[test]
    fn east_of_utc_morning_stays_on_the_previous_utc_day() {
        // 01:00+09 on Aug 25 is 16:00 UTC on Aug 24.
        assert_eq!(
            utc_calendar_date("2026-08-25T01:00:00+09:00").as_deref(),
            Some("2026-08-24")
        );
        assert!(!drift_is_due(
            "2026-08-25T01:00:00+09:00",
            "2026-08-24",
            "2026-08-25",
            0
        ));
    }
}
