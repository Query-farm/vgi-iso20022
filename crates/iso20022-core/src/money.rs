//! Exact money parsing for MT and MX amounts.
//!
//! All amounts flow through [`rust_decimal::Decimal`] — **never** `f64` — so the
//! MT103 <-> pacs.008 equivalence joins in the spec hold with exact equality and
//! no float drift. SWIFT MT amounts use a **comma** decimal separator (e.g.
//! `1234,56`); MX XML amounts use a dot (`1234.56`). Both are clamped to the
//! worker's `DECIMAL(38,9)` surface, which has headroom for every ISO 4217
//! minor-unit (including 3-dp BHD/KWD).

use rust_decimal::Decimal;
use std::str::FromStr;

/// The fixed scale of the worker's `DECIMAL(38,9)` money columns.
pub const MONEY_SCALE: u32 = 9;

/// Parse a SWIFT MT amount string (comma decimal separator) into a [`Decimal`].
///
/// SWIFT requires a comma and forbids a trailing decimal with no fraction in
/// most fields, but real files are messier — we normalize `,` -> `.`, strip
/// spaces, and tolerate a trailing separator. Returns `None` if the remainder is
/// not a valid decimal.
pub fn parse_mt_amount(s: &str) -> Option<Decimal> {
    let cleaned: String = s.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() {
        return None;
    }
    let dotted = cleaned.replace(',', ".");
    // A bare trailing dot ("1234.") parses in some libs but not rust_decimal;
    // drop it so "1234," -> "1234." -> "1234".
    let dotted = dotted.strip_suffix('.').unwrap_or(&dotted);
    Decimal::from_str(dotted).ok().map(clamp_scale)
}

/// Parse an MX XML amount string (dot decimal separator) into a [`Decimal`].
pub fn parse_mx_amount(s: &str) -> Option<Decimal> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    Decimal::from_str(t).ok().map(clamp_scale)
}

/// Parse a generic rate / numeric (exchange rate `:36:` / `XchgRate`).
pub fn parse_rate(s: &str) -> Option<Decimal> {
    let t: String = s.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if t.is_empty() {
        return None;
    }
    Decimal::from_str(&t.replace(',', "."))
        .ok()
        .map(clamp_scale)
}

/// Clamp a decimal to at most [`MONEY_SCALE`] fractional digits so it fits a
/// `DECIMAL(38,9)` column. Values with more precision are rounded half-up; this
/// only ever bites pathological inputs (real payment amounts are <= 5 dp).
fn clamp_scale(mut d: Decimal) -> Decimal {
    if d.scale() > MONEY_SCALE {
        d.rescale(MONEY_SCALE);
    }
    d
}

/// Convert a [`Decimal`] to the `i128` mantissa of a `DECIMAL(38,9)` value
/// (scaled by 10^9). Returns `None` if the value does not fit `DECIMAL(38,9)`.
///
/// This is the single bridge the worker uses to build an Arrow `Decimal128(38,9)`
/// array, keeping the scale contract in one place.
pub fn to_decimal128_i128(d: Decimal) -> Option<i128> {
    let mut d = d;
    d.rescale(MONEY_SCALE);
    // After rescale to 9, mantissa() is value * 10^9.
    let m = d.mantissa();
    // DECIMAL(38,9) range: |mantissa| < 10^38.
    const LIMIT: i128 = 10i128.pow(38);
    if m > -LIMIT && m < LIMIT {
        Some(m)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mt_comma_decimal() {
        assert_eq!(
            parse_mt_amount("1234,56"),
            Some(Decimal::from_str("1234.56").unwrap())
        );
        assert_eq!(parse_mt_amount("0,00"), Some(Decimal::ZERO));
        assert_eq!(parse_mt_amount("100,"), Some(Decimal::from(100)));
        assert_eq!(parse_mt_amount(""), None);
        assert_eq!(parse_mt_amount("abc"), None);
    }

    #[test]
    fn mx_dot_decimal() {
        assert_eq!(
            parse_mx_amount("1234.56"),
            Some(Decimal::from_str("1234.56").unwrap())
        );
        assert_eq!(
            parse_mx_amount(" 9.999 "),
            Some(Decimal::from_str("9.999").unwrap())
        );
    }

    #[test]
    fn exact_no_float_drift() {
        // 0.1 + 0.2 == 0.3 exactly with Decimal (the float-drift guard).
        let a = parse_mx_amount("0.1").unwrap();
        let b = parse_mx_amount("0.2").unwrap();
        assert_eq!(a + b, parse_mx_amount("0.3").unwrap());
    }

    #[test]
    fn decimal128_bridge() {
        let d = parse_mx_amount("1234.56").unwrap();
        assert_eq!(to_decimal128_i128(d), Some(1_234_560_000_000));
        assert_eq!(to_decimal128_i128(Decimal::ZERO), Some(0));
        // 3-dp currency (BHD) round-trips.
        let bhd = parse_mx_amount("12.345").unwrap();
        assert_eq!(to_decimal128_i128(bhd), Some(12_345_000_000));
    }
}
