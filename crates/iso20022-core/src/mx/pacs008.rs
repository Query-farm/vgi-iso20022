//! pacs.008 — FIToFICstmrCdtTrf (`pacs008_read`, one row per `CdtTrfTxInf`).

use super::common::{collect_texts, date_at, decimal_at, money_at};
use super::dom::{self, Node};
use chrono::{DateTime, FixedOffset};
use rust_decimal::Decimal;

/// One `CdtTrfTxInf` flattened to a row (group-header fields carried down).
#[derive(Debug, Clone, Default)]
pub struct Pacs008 {
    pub msg_id: Option<String>,
    pub creation_dt: Option<DateTime<FixedOffset>>,
    pub nb_of_txs: Option<i32>,
    pub settlement_method: Option<String>,
    pub instr_id: Option<String>,
    pub end_to_end_id: Option<String>,
    pub tx_id: Option<String>,
    pub uetr: Option<String>,
    pub amount: Option<Decimal>,
    pub ccy: Option<String>,
    pub settlement_date: Option<chrono::NaiveDate>,
    pub instructed_amount: Option<Decimal>,
    pub instructed_ccy: Option<String>,
    pub exchange_rate: Option<Decimal>,
    pub charge_bearer: Option<String>,
    pub debtor_name: Option<String>,
    pub debtor_iban: Option<String>,
    pub debtor_acct_other: Option<String>,
    pub debtor_agent_bic: Option<String>,
    pub creditor_name: Option<String>,
    pub creditor_iban: Option<String>,
    pub creditor_acct_other: Option<String>,
    pub creditor_agent_bic: Option<String>,
    pub intermediary_agent1_bic: Option<String>,
    pub purpose_code: Option<String>,
    pub remittance_unstructured: Vec<String>,
    pub remittance_struct_ref: Option<String>,
}

/// Parse a pacs.008 document into one row per `CdtTrfTxInf`.
pub fn parse(xml: &str) -> Vec<Pacs008> {
    let Some(root) = dom::parse(xml) else {
        return Vec::new();
    };
    let Some(body) = root.find_first("FIToFICstmrCdtTrf") else {
        return Vec::new();
    };
    let grp = body.child("GrpHdr");
    let msg_id = grp.and_then(|g| g.text_at(&["MsgId"]));
    let creation_dt = grp.and_then(|g| super::common::datetime_at(g, &["CreDtTm"]));
    let nb_of_txs = grp.and_then(|g| super::common::int_at(g, &["NbOfTxs"]));
    let settlement_method = grp.and_then(|g| g.text_at(&["SttlmInf", "SttlmMtd"]));

    body.children_named("CdtTrfTxInf")
        .map(|tx| {
            let amt = money_at(tx, &["IntrBkSttlmAmt"]);
            let instd = money_at(tx, &["InstdAmt"]);
            Pacs008 {
                msg_id: msg_id.clone(),
                creation_dt,
                nb_of_txs,
                settlement_method: settlement_method.clone(),
                instr_id: tx.text_at(&["PmtId", "InstrId"]),
                end_to_end_id: tx.text_at(&["PmtId", "EndToEndId"]),
                tx_id: tx.text_at(&["PmtId", "TxId"]),
                uetr: tx.text_at(&["PmtId", "UETR"]),
                amount: amt.amount,
                ccy: amt.ccy,
                settlement_date: date_at(tx, &["IntrBkSttlmDt"]),
                instructed_amount: instd.amount,
                instructed_ccy: instd.ccy,
                exchange_rate: decimal_at(tx, &["XchgRate"]),
                charge_bearer: tx.text_at(&["ChrgBr"]),
                debtor_name: tx.text_at(&["Dbtr", "Nm"]),
                debtor_iban: tx.text_at(&["DbtrAcct", "Id", "IBAN"]),
                debtor_acct_other: tx.text_at(&["DbtrAcct", "Id", "Othr", "Id"]),
                debtor_agent_bic: tx.text_at(&["DbtrAgt", "FinInstnId", "BICFI"]),
                creditor_name: tx.text_at(&["Cdtr", "Nm"]),
                creditor_iban: tx.text_at(&["CdtrAcct", "Id", "IBAN"]),
                creditor_acct_other: tx.text_at(&["CdtrAcct", "Id", "Othr", "Id"]),
                creditor_agent_bic: tx.text_at(&["CdtrAgt", "FinInstnId", "BICFI"]),
                intermediary_agent1_bic: tx.text_at(&["IntrmyAgt1", "FinInstnId", "BICFI"]),
                purpose_code: tx.text_at(&["Purp", "Cd"]),
                remittance_unstructured: collect_texts(tx, &["RmtInf"], "Ustrd"),
                remittance_struct_ref: struct_ref(tx),
            }
        })
        .collect()
}

/// `RmtInf/Strd/CdtrRefInf/Ref`.
pub(crate) fn struct_ref(tx: &Node) -> Option<String> {
    tx.text_at(&["RmtInf", "Strd", "CdtrRefInf", "Ref"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const XML: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08">
<FIToFICstmrCdtTrf>
  <GrpHdr><MsgId>PACS-MSG-1</MsgId><CreDtTm>2026-01-01T10:00:00Z</CreDtTm><NbOfTxs>1</NbOfTxs>
    <SttlmInf><SttlmMtd>INDA</SttlmMtd></SttlmInf></GrpHdr>
  <CdtTrfTxInf>
    <PmtId><InstrId>INSTR-1</InstrId><EndToEndId>E2E-REF-001</EndToEndId><UETR>e3bf1c2a-1111-4aaa-8bbb-1234567890ab</UETR></PmtId>
    <IntrBkSttlmAmt Ccy="EUR">1234.56</IntrBkSttlmAmt>
    <IntrBkSttlmDt>2026-01-01</IntrBkSttlmDt>
    <ChrgBr>SHAR</ChrgBr>
    <Dbtr><Nm>ACME CORP</Nm></Dbtr>
    <DbtrAcct><Id><IBAN>DE89370400440532013000</IBAN></Id></DbtrAcct>
    <DbtrAgt><FinInstnId><BICFI>DEUTDEFF</BICFI></FinInstnId></DbtrAgt>
    <Cdtr><Nm>WIDGETS SARL</Nm></Cdtr>
    <CdtrAcct><Id><IBAN>FR1420041010050500013M02606</IBAN></Id></CdtrAcct>
    <CdtrAgt><FinInstnId><BICFI>BNPAFRPP</BICFI></FinInstnId></CdtrAgt>
    <Purp><Cd>GDDS</Cd></Purp>
    <RmtInf><Ustrd>INVOICE 998877</Ustrd><Ustrd>PO 12345</Ustrd></RmtInf>
  </CdtTrfTxInf>
</FIToFICstmrCdtTrf></Document>"#;

    #[test]
    fn parses_one_tx() {
        let rows = parse(XML);
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.msg_id.as_deref(), Some("PACS-MSG-1"));
        assert_eq!(r.settlement_method.as_deref(), Some("INDA"));
        assert_eq!(r.end_to_end_id.as_deref(), Some("E2E-REF-001"));
        assert_eq!(r.amount, Some(Decimal::from_str("1234.56").unwrap()));
        assert_eq!(r.ccy.as_deref(), Some("EUR"));
        assert_eq!(r.debtor_iban.as_deref(), Some("DE89370400440532013000"));
        assert_eq!(r.creditor_name.as_deref(), Some("WIDGETS SARL"));
        assert_eq!(r.creditor_agent_bic.as_deref(), Some("BNPAFRPP"));
        assert_eq!(r.purpose_code.as_deref(), Some("GDDS"));
        assert_eq!(
            r.remittance_unstructured,
            vec!["INVOICE 998877", "PO 12345"]
        );
    }
}
