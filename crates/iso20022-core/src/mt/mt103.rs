//! MT103 — single customer credit transfer (`mt103_read`).

use super::block::{self, Block4};
use super::field::{self, parse_32a, parse_ccy_amount, parse_party};
use super::uetr;
use crate::charset;
use crate::money::parse_rate;
use chrono::NaiveDate;
use rust_decimal::Decimal;

/// One parsed MT103 message. `raw` / `path` provenance is added by the worker.
#[derive(Debug, Clone, Default)]
pub struct Mt103 {
    pub senders_ref: Option<String>,
    pub bank_op_code: Option<String>,
    pub instruction_codes: Vec<String>,
    pub value_date: Option<NaiveDate>,
    pub ccy: Option<String>,
    pub amount: Option<Decimal>,
    pub instructed_ccy: Option<String>,
    pub instructed_amount: Option<Decimal>,
    pub exchange_rate: Option<Decimal>,
    pub ordering_customer: Option<String>,
    pub ordering_customer_acct: Option<String>,
    pub ordering_customer_bic: Option<String>,
    pub ordering_institution: Option<String>,
    pub senders_correspondent: Option<String>,
    pub receivers_correspondent: Option<String>,
    pub third_reimbursement_inst: Option<String>,
    pub intermediary_inst: Option<String>,
    pub account_with_inst: Option<String>,
    pub beneficiary: Option<String>,
    pub beneficiary_acct: Option<String>,
    pub beneficiary_bic: Option<String>,
    pub remittance_info: Option<String>,
    pub details_of_charges: Option<String>,
    pub senders_charges: Vec<String>,
    pub receivers_charges: Option<String>,
    pub sender_to_receiver_info: Option<String>,
    pub end_to_end_id: Option<String>,
    pub uetr: Option<String>,
}

/// Parse a whole MT103 message. Returns a fully-populated struct (missing
/// optional fields stay `None`); it never fails — robustness is by design.
pub fn parse(msg: &str) -> Mt103 {
    let b4 = block::parse(msg);
    let uetr = uetr::extract(msg);
    let amt = b4.first("32A").map(parse_32a).unwrap_or(field::Amount32A {
        date: None,
        currency: None,
        amount: None,
    });
    let instr = b4.first("33B").map(parse_ccy_amount);
    let ordering = b4.first_prefix("50").map(|(_, v)| parse_party(v));
    let beneficiary = b4.first_prefix("59").map(|(_, v)| parse_party(v));

    let end_to_end_id = derive_end_to_end(&b4).or_else(|| uetr.clone());

    Mt103 {
        senders_ref: b4.first("20").map(san),
        bank_op_code: b4.first("23B").map(san),
        instruction_codes: b4.all("23E").into_iter().map(san).collect(),
        value_date: amt.date,
        ccy: amt.currency,
        amount: amt.amount,
        instructed_ccy: instr.as_ref().and_then(|a| a.currency.clone()),
        instructed_amount: instr.as_ref().and_then(|a| a.amount),
        exchange_rate: b4.first("36").and_then(parse_rate),
        ordering_customer: ordering.as_ref().and_then(|p| p.name_address.clone()),
        ordering_customer_acct: ordering.as_ref().and_then(|p| p.account.clone()),
        ordering_customer_bic: ordering.as_ref().and_then(|p| p.bic.clone()),
        ordering_institution: b4.first_prefix("52").map(|(_, v)| san(v)),
        senders_correspondent: b4.first_prefix("53").map(|(_, v)| san(v)),
        receivers_correspondent: b4.first_prefix("54").map(|(_, v)| san(v)),
        third_reimbursement_inst: b4.first_prefix("55").map(|(_, v)| san(v)),
        intermediary_inst: b4.first_prefix("56").map(|(_, v)| san(v)),
        account_with_inst: b4.first_prefix("57").map(|(_, v)| san(v)),
        beneficiary: beneficiary.as_ref().and_then(|p| p.name_address.clone()),
        beneficiary_acct: beneficiary.as_ref().and_then(|p| p.account.clone()),
        beneficiary_bic: beneficiary.as_ref().and_then(|p| p.bic.clone()),
        remittance_info: b4.first("70").map(san),
        details_of_charges: b4.first("71A").map(san),
        senders_charges: b4.all("71F").into_iter().map(san).collect(),
        receivers_charges: b4.first("71G").map(san),
        sender_to_receiver_info: b4.first("72").map(san),
        end_to_end_id,
        uetr,
    }
}

/// Derive an end-to-end id from the `:72:` sender-to-receiver info, recognizing
/// the structured `/EToE/` and `/UETR/` tokens used by MT->MX migrations.
pub(crate) fn derive_end_to_end(b4: &Block4) -> Option<String> {
    let info = b4.first("72")?;
    structured_token(info, "EToE")
        .or_else(|| structured_token(info, "ETOE"))
        .or_else(|| structured_token(info, "UETR"))
}

/// Extract the value following a `/TOKEN/` marker in a SWIFT structured field,
/// up to the next `/` or end of line.
pub(crate) fn structured_token(text: &str, token: &str) -> Option<String> {
    let needle = format!("/{token}/");
    let pos = text.find(&needle)? + needle.len();
    let tail = &text[pos..];
    let val: String = tail
        .chars()
        .take_while(|&c| c != '/' && c != '\n' && c != '\r')
        .collect();
    let val = val.trim().to_string();
    if val.is_empty() {
        None
    } else {
        Some(val)
    }
}

fn san(s: &str) -> String {
    charset::sanitize_x(s).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const MSG: &str = "{1:F01BANKBEBBAXXX0000000000}{2:I103DEUTDEFFXXXXN}{3:{121:e3bf1c2a-1111-4aaa-8bbb-1234567890ab}}{4:\n:20:TXN-REF-1\n:23B:CRED\n:32A:260101EUR1234,56\n:33B:USD1300,00\n:36:1,0525\n:50K:/DE89370400440532013000\nACME CORP\nBERLIN\n:59:/FR1420041010050500013M02606\nWIDGETS SARL\nPARIS\n:70:INVOICE 998877\n:71A:SHA\n:71F:EUR5,00\n:72:/EToE/E2E-REF-001\n-}";

    #[test]
    fn full_parse() {
        let m = parse(MSG);
        assert_eq!(m.senders_ref.as_deref(), Some("TXN-REF-1"));
        assert_eq!(m.bank_op_code.as_deref(), Some("CRED"));
        assert_eq!(m.amount, Some(Decimal::from_str("1234.56").unwrap()));
        assert_eq!(m.ccy.as_deref(), Some("EUR"));
        assert_eq!(
            m.instructed_amount,
            Some(Decimal::from_str("1300.00").unwrap())
        );
        assert_eq!(m.exchange_rate, Some(Decimal::from_str("1.0525").unwrap()));
        assert_eq!(
            m.ordering_customer_acct.as_deref(),
            Some("DE89370400440532013000")
        );
        assert_eq!(m.ordering_customer.as_deref(), Some("ACME CORP\nBERLIN"));
        assert_eq!(
            m.beneficiary_acct.as_deref(),
            Some("FR1420041010050500013M02606")
        );
        assert_eq!(m.remittance_info.as_deref(), Some("INVOICE 998877"));
        assert_eq!(m.details_of_charges.as_deref(), Some("SHA"));
        assert_eq!(m.senders_charges, vec!["EUR5,00"]);
        assert_eq!(
            m.uetr.as_deref(),
            Some("e3bf1c2a-1111-4aaa-8bbb-1234567890ab")
        );
        assert_eq!(m.end_to_end_id.as_deref(), Some("E2E-REF-001"));
    }

    #[test]
    fn end_to_end_falls_back_to_uetr() {
        let no72 = MSG.replace(":72:/EToE/E2E-REF-001\n", "");
        let m = parse(&no72);
        assert_eq!(m.end_to_end_id, m.uetr);
    }
}
