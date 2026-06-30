//! pain.001 — CstmrCdtTrfInitn (`pain001_read`, one row per `CdtTrfTxInf`,
//! `PmtInf` carried down).

use super::common::{collect_texts, datetime_at, decimal_at, money_at};
use super::dom::{self};
use chrono::{DateTime, FixedOffset, NaiveDate};
use rust_decimal::Decimal;

/// One `CdtTrfTxInf` flattened to a row, with its `PmtInf` parent carried down.
#[derive(Debug, Clone, Default)]
pub struct Pain001 {
    pub msg_id: Option<String>,
    pub creation_dt: Option<DateTime<FixedOffset>>,
    pub ctrl_sum: Option<Decimal>,
    pub initiating_party: Option<String>,
    pub pmt_inf_id: Option<String>,
    pub pmt_method: Option<String>,
    pub requested_exec_date: Option<NaiveDate>,
    pub debtor_name: Option<String>,
    pub debtor_iban: Option<String>,
    pub debtor_agent_bic: Option<String>,
    pub charge_bearer: Option<String>,
    pub end_to_end_id: Option<String>,
    pub instr_id: Option<String>,
    pub uetr: Option<String>,
    pub amount: Option<Decimal>,
    pub ccy: Option<String>,
    pub creditor_name: Option<String>,
    pub creditor_iban: Option<String>,
    pub creditor_agent_bic: Option<String>,
    pub purpose_code: Option<String>,
    pub remittance_unstructured: Vec<String>,
    pub remittance_struct_ref: Option<String>,
}

/// Parse a pain.001 document into one row per `CdtTrfTxInf` across all `PmtInf`.
pub fn parse(xml: &str) -> Vec<Pain001> {
    let Some(root) = dom::parse(xml) else {
        return Vec::new();
    };
    let Some(body) = root.find_first("CstmrCdtTrfInitn") else {
        return Vec::new();
    };
    let grp = body.child("GrpHdr");
    let msg_id = grp.and_then(|g| g.text_at(&["MsgId"]));
    let creation_dt = grp.and_then(|g| datetime_at(g, &["CreDtTm"]));
    let ctrl_sum = grp.and_then(|g| decimal_at(g, &["CtrlSum"]));
    let initiating_party = grp.and_then(|g| g.text_at(&["InitgPty", "Nm"]));

    let mut rows = Vec::new();
    for pmt in body.children_named("PmtInf") {
        let pmt_inf_id = pmt.text_at(&["PmtInfId"]);
        let pmt_method = pmt.text_at(&["PmtMtd"]);
        let requested_exec_date = pmt
            .text_at(&["ReqdExctnDt", "Dt"])
            .or_else(|| pmt.text_at(&["ReqdExctnDt"]))
            .as_deref()
            .and_then(crate::dates::parse_iso_date);
        let debtor_name = pmt.text_at(&["Dbtr", "Nm"]);
        let debtor_iban = pmt.text_at(&["DbtrAcct", "Id", "IBAN"]);
        let debtor_agent_bic = pmt.text_at(&["DbtrAgt", "FinInstnId", "BICFI"]);
        let charge_bearer = pmt.text_at(&["ChrgBr"]);

        for tx in pmt.children_named("CdtTrfTxInf") {
            let amt = money_at(tx, &["Amt", "InstdAmt"]);
            rows.push(Pain001 {
                msg_id: msg_id.clone(),
                creation_dt,
                ctrl_sum,
                initiating_party: initiating_party.clone(),
                pmt_inf_id: pmt_inf_id.clone(),
                pmt_method: pmt_method.clone(),
                requested_exec_date,
                debtor_name: debtor_name.clone(),
                debtor_iban: debtor_iban.clone(),
                debtor_agent_bic: debtor_agent_bic.clone(),
                charge_bearer: charge_bearer.clone(),
                end_to_end_id: tx.text_at(&["PmtId", "EndToEndId"]),
                instr_id: tx.text_at(&["PmtId", "InstrId"]),
                uetr: tx.text_at(&["PmtId", "UETR"]),
                amount: amt.amount,
                ccy: amt.ccy,
                creditor_name: tx.text_at(&["Cdtr", "Nm"]),
                creditor_iban: tx.text_at(&["CdtrAcct", "Id", "IBAN"]),
                creditor_agent_bic: tx.text_at(&["CdtrAgt", "FinInstnId", "BICFI"]),
                purpose_code: tx.text_at(&["Purp", "Cd"]),
                remittance_unstructured: collect_texts(tx, &["RmtInf"], "Ustrd"),
                remittance_struct_ref: tx.text_at(&["RmtInf", "Strd", "CdtrRefInf", "Ref"]),
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const XML: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pain.001.001.09">
<CstmrCdtTrfInitn>
  <GrpHdr><MsgId>PAIN-1</MsgId><CreDtTm>2026-01-01T09:00:00Z</CreDtTm><CtrlSum>1234.56</CtrlSum>
    <InitgPty><Nm>ACME CORP</Nm></InitgPty></GrpHdr>
  <PmtInf><PmtInfId>PMT-1</PmtInfId><PmtMtd>TRF</PmtMtd><ReqdExctnDt><Dt>2026-01-02</Dt></ReqdExctnDt>
    <Dbtr><Nm>ACME CORP</Nm></Dbtr>
    <DbtrAcct><Id><IBAN>DE89370400440532013000</IBAN></Id></DbtrAcct>
    <DbtrAgt><FinInstnId><BICFI>DEUTDEFF</BICFI></FinInstnId></DbtrAgt>
    <CdtTrfTxInf>
      <PmtId><EndToEndId>E2E-REF-001</EndToEndId></PmtId>
      <Amt><InstdAmt Ccy="EUR">1234.56</InstdAmt></Amt>
      <Cdtr><Nm>WIDGETS SARL</Nm></Cdtr>
      <CdtrAcct><Id><IBAN>FR1420041010050500013M02606</IBAN></Id></CdtrAcct>
      <RmtInf><Ustrd>INVOICE 998877</Ustrd></RmtInf>
    </CdtTrfTxInf>
  </PmtInf>
</CstmrCdtTrfInitn></Document>"#;

    #[test]
    fn parses_initiation() {
        let rows = parse(XML);
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.msg_id.as_deref(), Some("PAIN-1"));
        assert_eq!(r.ctrl_sum, Some(Decimal::from_str("1234.56").unwrap()));
        assert_eq!(r.pmt_inf_id.as_deref(), Some("PMT-1"));
        assert_eq!(r.pmt_method.as_deref(), Some("TRF"));
        assert_eq!(r.requested_exec_date, NaiveDate::from_ymd_opt(2026, 1, 2));
        assert_eq!(r.amount, Some(Decimal::from_str("1234.56").unwrap()));
        assert_eq!(r.end_to_end_id.as_deref(), Some("E2E-REF-001"));
        assert_eq!(
            r.creditor_iban.as_deref(),
            Some("FR1420041010050500013M02606")
        );
    }
}
