//! Date / timestamp parsing for MT (`YYMMDD` / `MMDD` with a century pivot) and
//! MX (ISO 8601 `ISODate` / `ISODateTime`).

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// Century pivot for two-digit SWIFT years: `00..=49` -> 2000..2049,
/// `50..=99` -> 1950..1999. This matches the conventional SWIFT/SR window.
fn pivot_year(yy: i32) -> i32 {
    if yy <= 49 {
        2000 + yy
    } else {
        1900 + yy
    }
}

/// Parse a SWIFT `YYMMDD` date (e.g. `:32A:` value date) with the 1950-2049
/// century pivot. Returns `None` on a malformed or out-of-range date.
pub fn parse_yymmdd(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let yy: i32 = s[0..2].parse().ok()?;
    let mm: u32 = s[2..4].parse().ok()?;
    let dd: u32 = s[4..6].parse().ok()?;
    NaiveDate::from_ymd_opt(pivot_year(yy), mm, dd)
}

/// Parse a SWIFT `MMDD` entry date (`:61:` subfield 2), inferring the year from a
/// neighbouring value date. Handles the year-boundary case (entry in January for
/// a December value date) by rolling the year forward one.
pub fn parse_mmdd(s: &str, value_date: Option<NaiveDate>) -> Option<NaiveDate> {
    use chrono::Datelike;
    let s = s.trim();
    if s.len() != 4 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mm: u32 = s[0..2].parse().ok()?;
    let dd: u32 = s[2..4].parse().ok()?;
    // The `:61:` entry date always sits alongside a value date (subfield 1 is
    // mandatory); without one we cannot infer the year, so return None rather
    // than guess (chrono carries no wall-clock here).
    let vd = value_date?;
    let base_year = vd.year();
    // Entry date months ahead of the value date's month by a wide margin means
    // the entry belongs to the previous year (value in Jan, entry booked the
    // prior Dec); months far behind => next year.
    if mm as i32 - vd.month() as i32 > 6 {
        return NaiveDate::from_ymd_opt(base_year - 1, mm, dd);
    }
    if vd.month() as i32 - mm as i32 > 6 {
        return NaiveDate::from_ymd_opt(base_year + 1, mm, dd);
    }
    NaiveDate::from_ymd_opt(base_year, mm, dd)
}

/// Parse an MX `ISODate` (`YYYY-MM-DD`). Tolerates a trailing time/zone by taking
/// the leading date.
pub fn parse_iso_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    let head = s.split(['T', ' ']).next().unwrap_or(s);
    NaiveDate::parse_from_str(head, "%Y-%m-%d").ok()
}

/// Parse an MX `ISODateTime` (`YYYY-MM-DDThh:mm:ss[.sss][Z|±hh:mm]`) into a
/// UTC-anchored [`DateTime`]. A naive datetime (no offset) is assumed UTC.
pub fn parse_iso_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt);
    }
    // RFC3339 without seconds-fraction but with offset is covered above; handle a
    // naive form (no zone) by assuming UTC.
    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(Utc.from_utc_datetime(&ndt).fixed_offset());
        }
    }
    // Date-only -> midnight UTC.
    if let Some(d) = parse_iso_date(s) {
        let ndt = d.and_hms_opt(0, 0, 0)?;
        return Some(Utc.from_utc_datetime(&ndt).fixed_offset());
    }
    None
}

/// Number of days since the Unix epoch (Arrow `Date32`). `None` if unrepresentable.
pub fn days_since_epoch(d: NaiveDate) -> Option<i32> {
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1)?;
    Some((d - epoch).num_days() as i32)
}

/// Microseconds since the Unix epoch in UTC (Arrow `Timestamp(Microsecond, UTC)`).
pub fn micros_since_epoch(dt: DateTime<FixedOffset>) -> i64 {
    dt.with_timezone(&Utc).timestamp_micros()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yymmdd_pivot() {
        assert_eq!(parse_yymmdd("260101"), NaiveDate::from_ymd_opt(2026, 1, 1));
        assert_eq!(
            parse_yymmdd("991231"),
            NaiveDate::from_ymd_opt(1999, 12, 31)
        );
        assert_eq!(parse_yymmdd("000229"), NaiveDate::from_ymd_opt(2000, 2, 29));
        assert_eq!(parse_yymmdd("261301"), None); // bad month
        assert_eq!(parse_yymmdd("2601"), None); // wrong length
    }

    #[test]
    fn mmdd_year_inference() {
        let vd = NaiveDate::from_ymd_opt(2026, 1, 5);
        // Entry 0103 with value 2026-01-05 -> same year.
        assert_eq!(parse_mmdd("0103", vd), NaiveDate::from_ymd_opt(2026, 1, 3));
        // Value in early Jan, entry in late Dec -> previous year.
        assert_eq!(
            parse_mmdd("1230", vd),
            NaiveDate::from_ymd_opt(2025, 12, 30)
        );
    }

    #[test]
    fn iso_dates() {
        assert_eq!(
            parse_iso_date("2026-06-29"),
            NaiveDate::from_ymd_opt(2026, 6, 29)
        );
        let dt = parse_iso_datetime("2026-06-29T10:11:12Z").unwrap();
        assert_eq!(
            dt.with_timezone(&Utc).to_rfc3339(),
            "2026-06-29T10:11:12+00:00"
        );
        assert!(parse_iso_datetime("2026-06-29T10:11:12+02:00").is_some());
        assert!(parse_iso_datetime("not a date").is_none());
    }
}
