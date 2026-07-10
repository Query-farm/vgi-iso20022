//! `pacs002_read(glob)` — one row per `TxInfAndSts`.

use std::sync::Arc;

use arrow_schema::{Schema, SchemaRef};
use iso20022_core::mx::pacs002 as core;

use super::common::ReadTable;
use super::scan::finish;
use crate::cols::*;

const EXAMPLES: &str = r#"[{"description":"Parse an inline pacs.002 payment status report: read the original payment references and the rejection reason down each TxInfAndSts row.","sql":"SELECT msg_id, orig_tx_id, orig_uetr, group_status FROM iso20022.main.pacs002_read('<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:pacs.002.001.10\"><FIToFIPmtStsRpt><GrpHdr><MsgId>STS-1</MsgId><CreDtTm>2026-01-01T11:00:00Z</CreDtTm></GrpHdr><OrgnlGrpInfAndSts><OrgnlMsgId>PACS-MSG-1</OrgnlMsgId><OrgnlMsgNmId>pacs.008.001.08</OrgnlMsgNmId></OrgnlGrpInfAndSts><TxInfAndSts><OrgnlEndToEndId>E2E-REF-001</OrgnlEndToEndId><OrgnlUETR>e3bf1c2a-1111-4aaa-8bbb-1234567890ab</OrgnlUETR><TxSts>RJCT</TxSts><StsRsnInf><Rsn><Cd>AM04</Cd></Rsn><AddtlInf>Insufficient funds</AddtlInf></StsRsnInf></TxInfAndSts></FIToFIPmtStsRpt></Document>') WHERE msg_id IS NOT NULL"}]"#;

/// The fixed output schema (column order matches [`build`]).
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented("msg_id", DataType::Utf8, "GrpHdr/MsgId."),
        commented("creation_dt", timestamp_type(), "GrpHdr/CreDtTm."),
        commented(
            "orig_msg_id",
            DataType::Utf8,
            "OrgnlGrpInfAndSts/OrgnlMsgId.",
        ),
        commented(
            "orig_msg_name",
            DataType::Utf8,
            "OrgnlGrpInfAndSts/OrgnlMsgNmId.",
        ),
        commented("group_status", DataType::Utf8, "OrgnlGrpInfAndSts/GrpSts."),
        commented("orig_instr_id", DataType::Utf8, "TxInfAndSts/OrgnlInstrId."),
        commented(
            "orig_end_to_end_id",
            DataType::Utf8,
            "TxInfAndSts/OrgnlEndToEndId.",
        ),
        commented("orig_tx_id", DataType::Utf8, "TxInfAndSts/OrgnlTxId."),
        commented("orig_uetr", DataType::Utf8, "TxInfAndSts/OrgnlUETR."),
        commented(
            "tx_status",
            DataType::Utf8,
            "TxInfAndSts/TxSts (ACSC/ACSP/RJCT/PDNG…).",
        ),
        commented(
            "status_reason_code",
            DataType::Utf8,
            "TxInfAndSts/StsRsnInf/Rsn/Cd.",
        ),
        commented(
            "status_reason_prop",
            DataType::Utf8,
            "TxInfAndSts/StsRsnInf/Rsn/Prtry.",
        ),
        commented(
            "status_reason_addtl",
            list_utf8_type(),
            "TxInfAndSts/StsRsnInf/AddtlInf (repeatable).",
        ),
        commented("accept_dt", timestamp_type(), "TxInfAndSts/AccptncDtTm."),
        commented("raw", DataType::Utf8, "The whole source document."),
        commented("path", DataType::Utf8, "Source file path."),
    ]))
}

/// Parse one file's pacs.002 statuses into a batch.
pub fn build(path: &str, content: &str) -> arrow_array::RecordBatch {
    let rows = core::parse(content);
    let s = schema();
    let cols = vec![
        str_col(rows.iter().map(|r| r.msg_id.clone())),
        ts_col(rows.iter().map(|r| r.creation_dt)),
        str_col(rows.iter().map(|r| r.orig_msg_id.clone())),
        str_col(rows.iter().map(|r| r.orig_msg_name.clone())),
        str_col(rows.iter().map(|r| r.group_status.clone())),
        str_col(rows.iter().map(|r| r.orig_instr_id.clone())),
        str_col(rows.iter().map(|r| r.orig_end_to_end_id.clone())),
        str_col(rows.iter().map(|r| r.orig_tx_id.clone())),
        str_col(rows.iter().map(|r| r.orig_uetr.clone())),
        str_col(rows.iter().map(|r| r.tx_status.clone())),
        str_col(rows.iter().map(|r| r.status_reason_code.clone())),
        str_col(rows.iter().map(|r| r.status_reason_prop.clone())),
        list_str_col(rows.iter().map(|r| r.status_reason_addtl.clone())),
        ts_col(rows.iter().map(|r| r.accept_dt)),
        str_col(rows.iter().map(|_| Some(content))),
        str_col(rows.iter().map(|_| Some(path))),
    ];
    finish(&s, cols)
}

/// The `pacs002_read` table-function descriptor.
pub fn table() -> ReadTable {
    ReadTable {
        name: "pacs002_read",
        schema,
        build,
        title: "Read pacs.002 Payment Statuses",
        doc_llm: "Scan a glob of ISO 20022 pacs.002 (FIToFIPmtStsRpt) XML files into one row per \
                  TxInfAndSts: the transaction status (ACSC/ACSP/RJCT/PDNG), status reason codes, \
                  and original references back to the pacs.008 it answers. Use it to reconcile \
                  payment acknowledgements / rejections.",
        doc_md: "Read pacs.002 payment-status reports into rows (one per TxInfAndSts).",
        keywords: "pacs.002, pacs002, FIToFIPmtStsRpt, payment status, ACSC, RJCT, reason code, \
                   acknowledgement, rejection, iso 20022",
        executable_examples: EXAMPLES,
    }
}
