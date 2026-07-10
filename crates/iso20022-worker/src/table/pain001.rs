//! `pain001_read(glob)` — one row per `CdtTrfTxInf` (PmtInf carried down).

use std::sync::Arc;

use arrow_schema::{Schema, SchemaRef};
use iso20022_core::mx::pain001 as core;

use super::common::ReadTable;
use super::scan::finish;
use crate::cols::*;

const EXAMPLES: &str = r#"[{"description":"Parse an inline pain.001 customer credit-transfer initiation: read the debtor, instructed amount, and creditor for each CdtTrfTxInf.","sql":"SELECT msg_id, pmt_method, debtor_name, amount, ccy, creditor_name FROM iso20022.main.pain001_read('<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:pain.001.001.09\"><CstmrCdtTrfInitn><GrpHdr><MsgId>PAIN-1</MsgId><CreDtTm>2026-01-01T09:00:00Z</CreDtTm><CtrlSum>1234.56</CtrlSum><InitgPty><Nm>ACME CORP</Nm></InitgPty></GrpHdr><PmtInf><PmtInfId>PMT-1</PmtInfId><PmtMtd>TRF</PmtMtd><ReqdExctnDt><Dt>2026-01-02</Dt></ReqdExctnDt><Dbtr><Nm>ACME CORP</Nm></Dbtr><DbtrAcct><Id><IBAN>DE89370400440532013000</IBAN></Id></DbtrAcct><DbtrAgt><FinInstnId><BICFI>DEUTDEFF</BICFI></FinInstnId></DbtrAgt><CdtTrfTxInf><PmtId><EndToEndId>E2E-REF-001</EndToEndId></PmtId><Amt><InstdAmt Ccy=\"EUR\">1234.56</InstdAmt></Amt><Cdtr><Nm>WIDGETS SARL</Nm></Cdtr><CdtrAcct><Id><IBAN>FR1420041010050500013M02606</IBAN></Id></CdtrAcct><RmtInf><Ustrd>INVOICE 998877</Ustrd></RmtInf></CdtTrfTxInf></PmtInf></CstmrCdtTrfInitn></Document>') WHERE amount > 1000"}]"#;

/// The fixed output schema (column order matches [`build`]).
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented("msg_id", DataType::Utf8, "GrpHdr/MsgId."),
        commented("creation_dt", timestamp_type(), "GrpHdr/CreDtTm."),
        commented("ctrl_sum", money_type(), "GrpHdr/CtrlSum."),
        commented("initiating_party", DataType::Utf8, "GrpHdr/InitgPty/Nm."),
        commented("pmt_inf_id", DataType::Utf8, "PmtInf/PmtInfId."),
        commented("pmt_method", DataType::Utf8, "PmtInf/PmtMtd (TRF/TRA/CHK)."),
        commented(
            "requested_exec_date",
            DataType::Date32,
            "PmtInf/ReqdExctnDt (or …/Dt).",
        ),
        commented("debtor_name", DataType::Utf8, "PmtInf/Dbtr/Nm."),
        commented("debtor_iban", DataType::Utf8, "PmtInf/DbtrAcct/Id/IBAN."),
        commented(
            "debtor_agent_bic",
            DataType::Utf8,
            "PmtInf/DbtrAgt/FinInstnId/BICFI.",
        ),
        commented("charge_bearer", DataType::Utf8, "PmtInf/ChrgBr."),
        commented(
            "end_to_end_id",
            DataType::Utf8,
            "CdtTrfTxInf/PmtId/EndToEndId.",
        ),
        commented("instr_id", DataType::Utf8, "CdtTrfTxInf/PmtId/InstrId."),
        commented("uetr", DataType::Utf8, "CdtTrfTxInf/PmtId/UETR."),
        commented("amount", money_type(), "CdtTrfTxInf/Amt/InstdAmt."),
        commented("ccy", DataType::Utf8, "Instructed amount currency (@Ccy)."),
        commented("creditor_name", DataType::Utf8, "CdtTrfTxInf/Cdtr/Nm."),
        commented(
            "creditor_iban",
            DataType::Utf8,
            "CdtTrfTxInf/CdtrAcct/Id/IBAN.",
        ),
        commented(
            "creditor_agent_bic",
            DataType::Utf8,
            "CdtTrfTxInf/CdtrAgt/FinInstnId/BICFI.",
        ),
        commented("purpose_code", DataType::Utf8, "CdtTrfTxInf/Purp/Cd."),
        commented(
            "remittance_unstructured",
            list_utf8_type(),
            "CdtTrfTxInf/RmtInf/Ustrd (repeatable).",
        ),
        commented(
            "remittance_struct_ref",
            DataType::Utf8,
            "CdtTrfTxInf/RmtInf/Strd/CdtrRefInf/Ref.",
        ),
        commented("raw", DataType::Utf8, "The whole source document."),
        commented("path", DataType::Utf8, "Source file path."),
    ]))
}

/// Parse one file's pain.001 transactions into a batch.
pub fn build(path: &str, content: &str) -> arrow_array::RecordBatch {
    let rows = core::parse(content);
    let s = schema();
    let cols = vec![
        str_col(rows.iter().map(|r| r.msg_id.clone())),
        ts_col(rows.iter().map(|r| r.creation_dt)),
        dec_col(rows.iter().map(|r| r.ctrl_sum)),
        str_col(rows.iter().map(|r| r.initiating_party.clone())),
        str_col(rows.iter().map(|r| r.pmt_inf_id.clone())),
        str_col(rows.iter().map(|r| r.pmt_method.clone())),
        date_col(rows.iter().map(|r| r.requested_exec_date)),
        str_col(rows.iter().map(|r| r.debtor_name.clone())),
        str_col(rows.iter().map(|r| r.debtor_iban.clone())),
        str_col(rows.iter().map(|r| r.debtor_agent_bic.clone())),
        str_col(rows.iter().map(|r| r.charge_bearer.clone())),
        str_col(rows.iter().map(|r| r.end_to_end_id.clone())),
        str_col(rows.iter().map(|r| r.instr_id.clone())),
        str_col(rows.iter().map(|r| r.uetr.clone())),
        dec_col(rows.iter().map(|r| r.amount)),
        str_col(rows.iter().map(|r| r.ccy.clone())),
        str_col(rows.iter().map(|r| r.creditor_name.clone())),
        str_col(rows.iter().map(|r| r.creditor_iban.clone())),
        str_col(rows.iter().map(|r| r.creditor_agent_bic.clone())),
        str_col(rows.iter().map(|r| r.purpose_code.clone())),
        list_str_col(rows.iter().map(|r| r.remittance_unstructured.clone())),
        str_col(rows.iter().map(|r| r.remittance_struct_ref.clone())),
        str_col(rows.iter().map(|_| Some(content))),
        str_col(rows.iter().map(|_| Some(path))),
    ];
    finish(&s, cols)
}

/// The `pain001_read` table-function descriptor.
pub fn table() -> ReadTable {
    ReadTable {
        name: "pain001_read",
        schema,
        build,
        title: "Read pain.001 Credit Initiations",
        doc_llm: "Scan a glob of ISO 20022 pain.001 (CstmrCdtTrfInitn) XML files into one row per \
                  CdtTrfTxInf, with the PmtInf debtor block carried down: control sum, requested \
                  execution date, debtor/creditor names + IBANs + BICs, amount and currency, \
                  end-to-end id, UETR, and remittance info. Use it to ingest outbound payment \
                  initiations.",
        doc_md:
            "Read pain.001 customer credit-transfer initiations into rows (one per CdtTrfTxInf).",
        keywords: "pain.001, pain001, CstmrCdtTrfInitn, credit initiation, payment initiation, \
                   iso 20022, debtor, creditor, iban, requested execution date, ctrl sum",
        executable_examples: EXAMPLES,
    }
}
