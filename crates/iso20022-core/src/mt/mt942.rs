//! MT942 — interim transaction report (`mt942_read` + `mt942_lines`).
//!
//! Same line shape as MT940 (`:61:`/`:86:`) but **without** booked opening/closing
//! balances; instead it carries floor limits (`:34F:`), a datetime indication
//! (`:13D:`), and debit/credit count + sum summaries (`:90D:`/`:90C:`).

use super::block::{self, Block4};
use super::mt940::{self, Line};
use crate::charset;
use crate::dates::parse_yymmdd;
use crate::money::parse_mt_amount;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use rust_decimal::Decimal;

/// One parsed MT942 interim report.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub statement_idx: i32,
    pub transaction_ref: Option<String>,
    pub related_ref: Option<String>,
    pub account: Option<String>,
    pub statement_no: Option<String>,
    pub sequence_no: Option<String>,
    pub floor_limit_debit: Option<Decimal>,
    pub floor_limit_credit: Option<Decimal>,
    pub ccy: Option<String>,
    pub datetime_indication: Option<DateTime<FixedOffset>>,
    pub debit_count: Option<i32>,
    pub debit_sum: Option<Decimal>,
    pub credit_count: Option<i32>,
    pub credit_sum: Option<Decimal>,
    pub line_count: i32,
    pub raw: String,
    pub lines: Vec<Line>,
}

/// Parse a whole MT942 file into reports (one per `:20:`).
pub fn parse_file(content: &str) -> Vec<Report> {
    let mut out = Vec::new();
    for body in mt940::block4_bodies(content) {
        let b4 = block::parse_body(&body);
        for group in mt940::split_statements(&b4.fields) {
            let gb4 = Block4 {
                fields: group.clone(),
            };
            let mut r = report_from_fields(&gb4);
            r.statement_idx = out.len() as i32;
            r.raw = mt940::serialize_fields(&group);
            out.push(r);
        }
    }
    out
}

fn report_from_fields(b4: &Block4) -> Report {
    let lines = mt940::parse_lines_from(b4);
    let (stmt_no, seq_no) = split_28c(b4.first("28C"));

    // Two :34F: floor limits: a mark-less one applies to both; otherwise D/C.
    let mut floor_debit = None;
    let mut floor_credit = None;
    let mut floor_ccy = None;
    for v in b4.all("34F") {
        let (ccy, dc, amt) = parse_34f(v);
        if floor_ccy.is_none() {
            floor_ccy = ccy;
        }
        match dc.as_deref() {
            Some("D") => floor_debit = amt,
            Some("C") => floor_credit = amt,
            _ => {
                floor_debit = amt;
                floor_credit = amt;
            }
        }
    }

    let (debit_count, debit_sum, dccy) =
        b4.first("90D").map(parse_90).unwrap_or((None, None, None));
    let (credit_count, credit_sum, cccy) =
        b4.first("90C").map(parse_90).unwrap_or((None, None, None));

    Report {
        statement_idx: 0,
        transaction_ref: b4.first("20").map(san),
        related_ref: b4.first("21").map(san),
        account: b4.first("25").map(san),
        statement_no: stmt_no,
        sequence_no: seq_no,
        floor_limit_debit: floor_debit,
        floor_limit_credit: floor_credit,
        ccy: floor_ccy.or(dccy).or(cccy),
        datetime_indication: b4.first("13D").and_then(parse_13d),
        debit_count,
        debit_sum,
        credit_count,
        credit_sum,
        line_count: lines.len() as i32,
        raw: String::new(),
        lines,
    }
}

/// Per-blob entry point: parse `:61:`/`:86:` lines from an MT942 blob (identical
/// to MT940's line model).
pub fn parse_lines(blob: &str) -> Vec<Line> {
    mt940::parse_lines(blob)
}

/// Parse `:34F:` `<CCY>[D|C]<amount>` into `(ccy, dc, amount)`.
fn parse_34f(value: &str) -> (Option<String>, Option<String>, Option<Decimal>) {
    let v = value.trim();
    let ccy = v.get(0..3).map(|s| s.to_string());
    let rest = v.get(3..).unwrap_or("");
    let (dc, amt_str) = match rest.as_bytes().first() {
        Some(b'D') => (Some("D".to_string()), &rest[1..]),
        Some(b'C') => (Some("C".to_string()), &rest[1..]),
        _ => (None, rest),
    };
    (ccy, dc, parse_mt_amount(amt_str))
}

/// Parse `:90D:`/`:90C:` `<count><CCY><amount>` into `(count, sum, ccy)`.
fn parse_90(value: &str) -> (Option<i32>, Option<Decimal>, Option<String>) {
    let v = value.trim();
    let count_str: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
    let count = count_str.parse::<i32>().ok();
    let rest = &v[count_str.len()..];
    let ccy = rest.get(0..3).map(|s| s.to_string());
    let sum = rest.get(3..).and_then(parse_mt_amount);
    (count, sum, ccy)
}

/// Parse `:13D:` `<YYMMDD><HHMM><sign><HHMM>` into a timezone-aware datetime.
fn parse_13d(value: &str) -> Option<DateTime<FixedOffset>> {
    let v = value.trim();
    if v.len() < 15 {
        return None;
    }
    let date: NaiveDate = parse_yymmdd(&v[0..6])?;
    let hh: u32 = v[6..8].parse().ok()?;
    let mm: u32 = v[8..10].parse().ok()?;
    let sign = &v[10..11];
    let off_h: i32 = v[11..13].parse().ok()?;
    let off_m: i32 = v[13..15].parse().ok()?;
    let mut offset_secs = off_h * 3600 + off_m * 60;
    if sign == "-" {
        offset_secs = -offset_secs;
    }
    let tz = FixedOffset::east_opt(offset_secs)?;
    let naive = date.and_hms_opt(hh, mm, 0)?;
    tz.from_local_datetime(&naive).single()
}

fn split_28c(value: Option<&str>) -> (Option<String>, Option<String>) {
    match value {
        None => (None, None),
        Some(v) => match v.split_once('/') {
            Some((n, s)) => (non_empty(n), non_empty(s)),
            None => (non_empty(v), None),
        },
    }
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn san(s: &str) -> String {
    charset::sanitize_x(s).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::str::FromStr;

    const MSG: &str = "{1:F01X}{2:O942X}{4:\n:20:INTERIM-1\n:25:DE89370400440532013000\n:28C:99/1\n:34F:EURD0,00\n:34F:EURC1000,00\n:13D:2601021430+0100\n:61:2601020102C500,00NTRFNONREF//BANK-A\n:86:INCOMING\n:90D:5EUR2500,00\n:90C:3EUR1500,00\n-}";

    #[test]
    fn interim_fields() {
        let rs = parse_file(MSG);
        assert_eq!(rs.len(), 1);
        let r = &rs[0];
        assert_eq!(r.transaction_ref.as_deref(), Some("INTERIM-1"));
        assert_eq!(r.floor_limit_debit, Some(Decimal::ZERO));
        assert_eq!(
            r.floor_limit_credit,
            Some(Decimal::from_str("1000.00").unwrap())
        );
        assert_eq!(r.debit_count, Some(5));
        assert_eq!(r.debit_sum, Some(Decimal::from_str("2500.00").unwrap()));
        assert_eq!(r.credit_count, Some(3));
        assert_eq!(r.line_count, 1);
        let dt = r.datetime_indication.unwrap();
        assert_eq!(
            dt.with_timezone(&Utc).to_rfc3339(),
            "2026-01-02T13:30:00+00:00"
        );
    }
}
