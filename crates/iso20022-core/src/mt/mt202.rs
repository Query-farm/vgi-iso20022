//! MT202 / MT202 COV — general financial-institution transfer (`mt202_read`).

use super::block::{self};
use super::field::{parse_32a, parse_party};
use super::uetr;
use crate::charset;
use chrono::NaiveDate;
use rust_decimal::Decimal;

/// One parsed MT202 (optionally COV) message.
#[derive(Debug, Clone, Default)]
pub struct Mt202 {
    pub transaction_ref: Option<String>,
    pub related_ref: Option<String>,
    pub value_date: Option<NaiveDate>,
    pub ccy: Option<String>,
    pub amount: Option<Decimal>,
    pub ordering_institution: Option<String>,
    pub senders_correspondent: Option<String>,
    pub receivers_correspondent: Option<String>,
    pub intermediary: Option<String>,
    pub account_with_inst: Option<String>,
    pub beneficiary_inst: Option<String>,
    pub sender_to_receiver_info: Option<String>,
    pub is_cover: bool,
    pub cov_ordering_customer: Option<String>,
    pub cov_beneficiary: Option<String>,
    pub cov_remittance_info: Option<String>,
    pub uetr: Option<String>,
}

/// Parse an MT202 / MT202 COV message. The COV underlying-customer block is the
/// sequence B fields (`:50a:`/`:59a:`/`:70:`) that follow the FI fields; their
/// presence flips `is_cover`.
pub fn parse(msg: &str) -> Mt202 {
    let b4 = block::parse(msg);
    let amt = b4.first("32A").map(parse_32a);

    // COV detection: an underlying customer (50a / 59a) is present in addition to
    // the FI beneficiary institution (58a).
    let is_cover = b4.has_prefix("50") || b4.has_prefix("59");
    let cov_ordering = b4.first_prefix("50").map(|(_, v)| parse_party(v));
    let cov_beneficiary = b4.first_prefix("59").map(|(_, v)| parse_party(v));

    Mt202 {
        transaction_ref: b4.first("20").map(san),
        related_ref: b4.first("21").map(san),
        value_date: amt.as_ref().and_then(|a| a.date),
        ccy: amt.as_ref().and_then(|a| a.currency.clone()),
        amount: amt.as_ref().and_then(|a| a.amount),
        ordering_institution: b4.first_prefix("52").map(|(_, v)| san(v)),
        senders_correspondent: b4.first_prefix("53").map(|(_, v)| san(v)),
        receivers_correspondent: b4.first_prefix("54").map(|(_, v)| san(v)),
        intermediary: b4.first_prefix("56").map(|(_, v)| san(v)),
        account_with_inst: b4.first_prefix("57").map(|(_, v)| san(v)),
        beneficiary_inst: b4.first_prefix("58").map(|(_, v)| san(v)),
        sender_to_receiver_info: b4.first("72").map(san),
        is_cover,
        cov_ordering_customer: cov_ordering.and_then(|p| p.name_address.or(p.account).or(p.bic)),
        cov_beneficiary: cov_beneficiary.and_then(|p| p.name_address.or(p.account).or(p.bic)),
        cov_remittance_info: b4.first("70").map(san),
        uetr: uetr::extract(msg),
    }
}

fn san(s: &str) -> String {
    charset::sanitize_x(s).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const COV: &str = "{1:F01X}{2:I202DEUTDEFFXXXXN}{3:{121:11111111-2222-4333-8444-555566667777}}{4:\n:20:FI-REF-9\n:21:REL-REF-9\n:32A:260102USD5000000,00\n:52A:CHASUS33\n:58A:DEUTDEFF\n:50K:/111\nUNDERLYING DEBTOR\n:59:/222\nUNDERLYING CREDITOR\n:70:COVER FOR MT103\n-}";

    #[test]
    fn cover_parse() {
        let m = parse(COV);
        assert_eq!(m.transaction_ref.as_deref(), Some("FI-REF-9"));
        assert_eq!(m.related_ref.as_deref(), Some("REL-REF-9"));
        assert_eq!(m.amount, Some(Decimal::from_str("5000000.00").unwrap()));
        assert!(m.is_cover);
        assert_eq!(
            m.cov_ordering_customer.as_deref(),
            Some("UNDERLYING DEBTOR")
        );
        assert_eq!(m.cov_beneficiary.as_deref(), Some("UNDERLYING CREDITOR"));
        assert_eq!(m.cov_remittance_info.as_deref(), Some("COVER FOR MT103"));
        assert_eq!(
            m.uetr.as_deref(),
            Some("11111111-2222-4333-8444-555566667777")
        );
    }

    #[test]
    fn plain_mt202_not_cover() {
        let plain = "{1:F01X}{2:I202X}{4:\n:20:R\n:21:REL\n:32A:260102USD100,00\n:58A:DEUTDEFF\n-}";
        let m = parse(plain);
        assert!(!m.is_cover);
        assert_eq!(m.beneficiary_inst.as_deref(), Some("DEUTDEFF"));
    }
}
