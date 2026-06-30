//! pacs.002 — FIToFIPmtStsRpt (`pacs002_read`, one row per `TxInfAndSts`).

use super::common::{collect_texts, datetime_at};
use super::dom::{self};
use chrono::{DateTime, FixedOffset};

/// One `TxInfAndSts` flattened to a row (group + original-group info carried down).
#[derive(Debug, Clone, Default)]
pub struct Pacs002 {
    pub msg_id: Option<String>,
    pub creation_dt: Option<DateTime<FixedOffset>>,
    pub orig_msg_id: Option<String>,
    pub orig_msg_name: Option<String>,
    pub group_status: Option<String>,
    pub orig_instr_id: Option<String>,
    pub orig_end_to_end_id: Option<String>,
    pub orig_tx_id: Option<String>,
    pub orig_uetr: Option<String>,
    pub tx_status: Option<String>,
    pub status_reason_code: Option<String>,
    pub status_reason_prop: Option<String>,
    pub status_reason_addtl: Vec<String>,
    pub accept_dt: Option<DateTime<FixedOffset>>,
}

/// Parse a pacs.002 document into one row per `TxInfAndSts`. When a report
/// carries only group-level status (no `TxInfAndSts`), a single group row is
/// emitted so the status is still visible.
pub fn parse(xml: &str) -> Vec<Pacs002> {
    let Some(root) = dom::parse(xml) else {
        return Vec::new();
    };
    let Some(body) = root.find_first("FIToFIPmtStsRpt") else {
        return Vec::new();
    };
    let grp = body.child("GrpHdr");
    let msg_id = grp.and_then(|g| g.text_at(&["MsgId"]));
    let creation_dt = grp.and_then(|g| datetime_at(g, &["CreDtTm"]));

    let ogi = body.child("OrgnlGrpInfAndSts");
    let orig_msg_id = ogi.and_then(|o| o.text_at(&["OrgnlMsgId"]));
    let orig_msg_name = ogi.and_then(|o| o.text_at(&["OrgnlMsgNmId"]));
    let group_status = ogi.and_then(|o| o.text_at(&["GrpSts"]));

    let base = Pacs002 {
        msg_id: msg_id.clone(),
        creation_dt,
        orig_msg_id: orig_msg_id.clone(),
        orig_msg_name: orig_msg_name.clone(),
        group_status: group_status.clone(),
        ..Default::default()
    };

    let rows: Vec<Pacs002> = body
        .children_named("TxInfAndSts")
        .map(|tx| {
            let rsn = tx.descend(&["StsRsnInf"]);
            Pacs002 {
                orig_instr_id: tx.text_at(&["OrgnlInstrId"]),
                orig_end_to_end_id: tx.text_at(&["OrgnlEndToEndId"]),
                orig_tx_id: tx.text_at(&["OrgnlTxId"]),
                orig_uetr: tx.text_at(&["OrgnlUETR"]),
                tx_status: tx.text_at(&["TxSts"]),
                status_reason_code: rsn.and_then(|r| r.text_at(&["Rsn", "Cd"])),
                status_reason_prop: rsn.and_then(|r| r.text_at(&["Rsn", "Prtry"])),
                status_reason_addtl: rsn
                    .map(|r| collect_texts(r, &[], "AddtlInf"))
                    .unwrap_or_default(),
                accept_dt: datetime_at(tx, &["AccptncDtTm"]),
                ..base.clone()
            }
        })
        .collect();

    if rows.is_empty() {
        vec![base]
    } else {
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.002.001.10">
<FIToFIPmtStsRpt>
  <GrpHdr><MsgId>STS-1</MsgId><CreDtTm>2026-01-01T11:00:00Z</CreDtTm></GrpHdr>
  <OrgnlGrpInfAndSts><OrgnlMsgId>PACS-MSG-1</OrgnlMsgId><OrgnlMsgNmId>pacs.008.001.08</OrgnlMsgNmId></OrgnlGrpInfAndSts>
  <TxInfAndSts>
    <OrgnlEndToEndId>E2E-REF-001</OrgnlEndToEndId>
    <OrgnlUETR>e3bf1c2a-1111-4aaa-8bbb-1234567890ab</OrgnlUETR>
    <TxSts>RJCT</TxSts>
    <StsRsnInf><Rsn><Cd>AM04</Cd></Rsn><AddtlInf>Insufficient funds</AddtlInf></StsRsnInf>
  </TxInfAndSts>
</FIToFIPmtStsRpt></Document>"#;

    #[test]
    fn parses_status() {
        let rows = parse(XML);
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.msg_id.as_deref(), Some("STS-1"));
        assert_eq!(r.orig_msg_id.as_deref(), Some("PACS-MSG-1"));
        assert_eq!(r.orig_end_to_end_id.as_deref(), Some("E2E-REF-001"));
        assert_eq!(r.tx_status.as_deref(), Some("RJCT"));
        assert_eq!(r.status_reason_code.as_deref(), Some("AM04"));
        assert_eq!(r.status_reason_addtl, vec!["Insufficient funds"]);
    }
}
