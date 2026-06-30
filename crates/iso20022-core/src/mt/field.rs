//! Shared MT subfield parsing: party blocks (`:50a:` / `:59a:`), amount fields
//! (`:32A:` / `:33B:`), and the raw-tag accessor behind `iso20022_mt103_field`.

use crate::charset;
use crate::dates::parse_yymmdd;
use crate::money::parse_mt_amount;
use chrono::NaiveDate;
use rust_decimal::Decimal;

/// A decomposed party field (`:50A/F/K:`, `:59/59A/59F:`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Party {
    /// Account / IBAN line (`/…`), without the leading slash, sanitized.
    pub account: Option<String>,
    /// Name + address lines joined with `\n`, sanitized.
    pub name_address: Option<String>,
    /// BIC, when the option carries one (option A).
    pub bic: Option<String>,
}

/// Parse a party field value into `(account, name_address, bic)`. The first line
/// beginning with `/` is the account/IBAN; a line matching a BIC pattern is the
/// BIC (option A); the remaining lines are the name + address.
pub fn parse_party(value: &str) -> Party {
    let mut account = None;
    let mut bic = None;
    let mut name_lines: Vec<String> = Vec::new();

    for (i, line) in value.split('\n').enumerate() {
        let line = line.trim_end_matches('\r');
        if i == 0 && line.starts_with('/') {
            // /account  or  //account
            let acct = line.trim_start_matches('/').trim();
            if !acct.is_empty() {
                account = Some(charset::sanitize_x(acct).0);
            }
            continue;
        }
        if bic.is_none() && is_bic(line.trim()) {
            bic = Some(line.trim().to_string());
            continue;
        }
        let (clean, _) = charset::sanitize_x(line);
        name_lines.push(clean);
    }

    let name_address = if name_lines.is_empty() {
        None
    } else {
        Some(name_lines.join("\n"))
    };
    Party {
        account,
        name_address,
        bic,
    }
}

/// Is `s` a syntactically valid BIC (8 or 11): 6 letters, 2 alnum, optional 3 alnm.
pub fn is_bic(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 8 && b.len() != 11 {
        return false;
    }
    b[0..6].iter().all(|c| c.is_ascii_uppercase())
        && b[6..8]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() && !c.is_ascii_lowercase())
        && b[8..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() && !c.is_ascii_lowercase())
}

/// A parsed `:32A:` interbank-settled amount field: value date, currency, amount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Amount32A {
    pub date: Option<NaiveDate>,
    pub currency: Option<String>,
    pub amount: Option<Decimal>,
}

/// Parse a `:32A:` value (`YYMMDD` + 3-char currency + comma-decimal amount).
pub fn parse_32a(value: &str) -> Amount32A {
    let v = value.trim();
    let date = v.get(0..6).and_then(parse_yymmdd);
    let currency = v.get(6..9).map(|c| c.to_string());
    let amount = v.get(9..).and_then(parse_mt_amount);
    Amount32A {
        date,
        currency,
        amount,
    }
}

/// A parsed `:33B:` instructed amount (currency + comma-decimal amount, no date).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmountCcy {
    pub currency: Option<String>,
    pub amount: Option<Decimal>,
}

/// Parse a `:33B:`-style value (3-char currency + amount).
pub fn parse_ccy_amount(value: &str) -> AmountCcy {
    let v = value.trim();
    let currency = v.get(0..3).map(|c| c.to_string());
    let amount = v.get(3..).and_then(parse_mt_amount);
    AmountCcy { currency, amount }
}

/// The raw text of any tag, for `iso20022_mt103_field(blob, tag)`. Repeatable
/// tags (e.g. `71F`) are joined with `\n`. Returns `None` if the tag is absent.
pub fn raw_tag(b4: &crate::mt::block::Block4, tag: &str) -> Option<String> {
    let matches = b4.all(tag);
    if matches.is_empty() {
        // Allow a bare numeric prefix (e.g. "50") to match an option variant.
        if let Some((_, v)) = b4.first_prefix(tag) {
            return Some(v.to_string());
        }
        return None;
    }
    Some(matches.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn party_50k() {
        let p = parse_party("/12345678\nACME CORP\n123 MAIN ST");
        assert_eq!(p.account.as_deref(), Some("12345678"));
        assert_eq!(p.name_address.as_deref(), Some("ACME CORP\n123 MAIN ST"));
        assert!(p.bic.is_none());
    }

    #[test]
    fn party_50a_bic() {
        let p = parse_party("/12345678\nDEUTDEFFXXX");
        assert_eq!(p.account.as_deref(), Some("12345678"));
        assert_eq!(p.bic.as_deref(), Some("DEUTDEFFXXX"));
    }

    #[test]
    fn bic_validation() {
        assert!(is_bic("DEUTDEFF"));
        assert!(is_bic("DEUTDEFFXXX"));
        assert!(!is_bic("deutdeff"));
        assert!(!is_bic("DEUT1EFF")); // digit in bank-code position
        assert!(!is_bic("SHORT"));
    }

    #[test]
    fn amount_32a() {
        let a = parse_32a("260101EUR1234,56");
        assert_eq!(a.date, NaiveDate::from_ymd_opt(2026, 1, 1));
        assert_eq!(a.currency.as_deref(), Some("EUR"));
        assert_eq!(a.amount, Some(Decimal::from_str("1234.56").unwrap()));
    }

    #[test]
    fn amount_33b() {
        let a = parse_ccy_amount("USD999,99");
        assert_eq!(a.currency.as_deref(), Some("USD"));
        assert_eq!(a.amount, Some(Decimal::from_str("999.99").unwrap()));
    }
}
