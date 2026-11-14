//! camt.054 — BkToCstmrDbtCdtNtfctn (`camt054_read` + `camt054_entries`).
//!
//! A notification shares the camt.053 `Stmt`/`Ntry` shape but the parent is
//! `Ntfctn`; the header reuses `camt053::statement_of` and the entries reuse
//! the shared `camt::entries_of`. Balances are usually absent on a
//! notification but are surfaced when present.

use super::camt::{entries_of, Entry};
use super::camt053::{statement_of, Statement};
use super::common::datetime_at;
use super::dom::{self};

/// Parse a camt.054 document into one row per `Ntfctn`.
pub fn parse(xml: &str) -> Vec<Statement> {
    let Some(root) = dom::parse(xml) else {
        return Vec::new();
    };
    let Some(body) = root.find_first("BkToCstmrDbtCdtNtfctn") else {
        return Vec::new();
    };
    let grp = body.child("GrpHdr");
    let msg_id = grp.and_then(|g| g.text_at(&["MsgId"]));
    let creation_dt = grp.and_then(|g| datetime_at(g, &["CreDtTm"]));

    let docs = dom::split_children(xml, "BkToCstmrDbtCdtNtfctn", "Ntfctn");
    body.children_named("Ntfctn")
        .enumerate()
        .map(|(i, n)| Statement {
            raw: docs.get(i).cloned().unwrap_or_else(|| xml.to_string()),
            ..statement_of(n, msg_id.clone(), creation_dt)
        })
        .collect()
}

/// Per-blob entry point: parse `Ntry` rows from a whole camt.054 document.
pub fn parse_entries(xml: &str) -> Vec<Entry> {
    let Some(root) = dom::parse(xml) else {
        return Vec::new();
    };
    match root.find_first("Ntfctn") {
        Some(n) => entries_of(n),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    const XML: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.054.001.08">
<BkToCstmrDbtCdtNtfctn>
  <GrpHdr><MsgId>CAMT054-1</MsgId><CreDtTm>2026-01-03T06:30:00Z</CreDtTm></GrpHdr>
  <Ntfctn><Id>NTF-1</Id>
    <Acct><Id><IBAN>DE89370400440532013000</IBAN></Id><Ccy>EUR</Ccy></Acct>
    <Ntry><Amt Ccy="EUR">250.50</Amt><CdtDbtInd>DBIT</CdtDbtInd><Sts><Cd>BOOK</Cd></Sts>
      <NtryDtls><TxDtls>
        <Refs><EndToEndId>E2E-DEBIT-9</EndToEndId></Refs>
        <RltdPties><Cdtr><Nm>UTILITY CO</Nm></Cdtr><CdtrAcct><Id><IBAN>FR1420041010050500013M02606</IBAN></Id></CdtrAcct></RltdPties>
      </TxDtls></NtryDtls>
    </Ntry>
  </Ntfctn>
</BkToCstmrDbtCdtNtfctn></Document>"#;

    #[test]
    fn parses_notification_header() {
        let s = parse(XML);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].msg_id.as_deref(), Some("CAMT054-1"));
        assert_eq!(s[0].stmt_id.as_deref(), Some("NTF-1"));
        assert_eq!(s[0].account_iban.as_deref(), Some("DE89370400440532013000"));
        assert_eq!(s[0].entry_count, 1);
    }

    #[test]
    fn parses_debit_entry_counterparty_is_creditor() {
        let e = parse_entries(XML);
        assert_eq!(e.len(), 1);
        let r = &e[0];
        assert_eq!(r.credit_debit.as_deref(), Some("DBIT"));
        assert_eq!(r.amount, Some(Decimal::from_str("250.50").unwrap()));
        // DBIT -> counterparty is the creditor.
        assert_eq!(r.counterparty_name.as_deref(), Some("UTILITY CO"));
        assert_eq!(
            r.counterparty_iban.as_deref(),
            Some("FR1420041010050500013M02606")
        );
        assert_eq!(r.end_to_end_id.as_deref(), Some("E2E-DEBIT-9"));
    }

    #[test]
    fn multi_notification_raw_is_narrowed_to_each_notification() {
        let xml = r#"<Document><BkToCstmrDbtCdtNtfctn><GrpHdr><MsgId>M</MsgId></GrpHdr><Ntfctn><Id>N1</Id><Ntry><Amt Ccy="EUR">1.00</Amt><CdtDbtInd>DBIT</CdtDbtInd></Ntry></Ntfctn><Ntfctn><Id>N2</Id><Ntry><Amt Ccy="EUR">2.00</Amt><CdtDbtInd>CRDT</CdtDbtInd></Ntry></Ntfctn></BkToCstmrDbtCdtNtfctn></Document>"#;
        let s = parse(xml);
        assert_eq!(s.len(), 2);
        let e1 = parse_entries(&s[0].raw);
        let e2 = parse_entries(&s[1].raw);
        assert_eq!(e1.len(), 1);
        assert_eq!(e1[0].credit_debit.as_deref(), Some("DBIT"));
        assert_eq!(e2.len(), 1);
        assert_eq!(e2[0].credit_debit.as_deref(), Some("CRDT"));
    }
}
