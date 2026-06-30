//! `pacs008_read(glob)` — one row per `CdtTrfTxInf`.

use std::sync::Arc;

use arrow_schema::{Schema, SchemaRef};
use iso20022_core::mx::pacs008 as core;

use super::common::ReadTable;
use super::scan::finish;
use crate::cols::*;

const RESULT_MD: &str =
    "One row per `CdtTrfTxInf` (group-header fields carried down). Key columns: \
`msg_id`, `end_to_end_id`, `uetr`, `amount` DECIMAL(38,9) + `ccy`, debtor/creditor `*_name` / \
`*_iban` / `*_agent_bic`, `charge_bearer`, `purpose_code`, `remittance_unstructured` VARCHAR[], \
plus `raw` (whole document) and `path` provenance.";

/// The fixed output schema (column order matches [`build`]).
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented("msg_id", DataType::Utf8, "GrpHdr/MsgId."),
        commented("creation_dt", timestamp_type(), "GrpHdr/CreDtTm."),
        commented("nb_of_txs", DataType::Int32, "GrpHdr/NbOfTxs."),
        commented(
            "settlement_method",
            DataType::Utf8,
            "GrpHdr/SttlmInf/SttlmMtd (INDA/INGA/CLRG/COVE).",
        ),
        commented("instr_id", DataType::Utf8, "CdtTrfTxInf/PmtId/InstrId."),
        commented(
            "end_to_end_id",
            DataType::Utf8,
            "CdtTrfTxInf/PmtId/EndToEndId.",
        ),
        commented("tx_id", DataType::Utf8, "CdtTrfTxInf/PmtId/TxId."),
        commented("uetr", DataType::Utf8, "CdtTrfTxInf/PmtId/UETR."),
        commented(
            "amount",
            money_type(),
            "Interbank settled amount (IntrBkSttlmAmt).",
        ),
        commented("ccy", DataType::Utf8, "Settled amount currency (@Ccy)."),
        commented(
            "settlement_date",
            DataType::Date32,
            "CdtTrfTxInf/IntrBkSttlmDt.",
        ),
        commented("instructed_amount", money_type(), "CdtTrfTxInf/InstdAmt."),
        commented(
            "instructed_ccy",
            DataType::Utf8,
            "Instructed amount currency (@Ccy).",
        ),
        commented("exchange_rate", money_type(), "CdtTrfTxInf/XchgRate."),
        commented(
            "charge_bearer",
            DataType::Utf8,
            "CdtTrfTxInf/ChrgBr (DEBT/CRED/SHAR/SLEV).",
        ),
        commented("debtor_name", DataType::Utf8, "CdtTrfTxInf/Dbtr/Nm."),
        commented(
            "debtor_iban",
            DataType::Utf8,
            "CdtTrfTxInf/DbtrAcct/Id/IBAN.",
        ),
        commented(
            "debtor_acct_other",
            DataType::Utf8,
            "CdtTrfTxInf/DbtrAcct/Id/Othr/Id.",
        ),
        commented(
            "debtor_agent_bic",
            DataType::Utf8,
            "CdtTrfTxInf/DbtrAgt/FinInstnId/BICFI.",
        ),
        commented("creditor_name", DataType::Utf8, "CdtTrfTxInf/Cdtr/Nm."),
        commented(
            "creditor_iban",
            DataType::Utf8,
            "CdtTrfTxInf/CdtrAcct/Id/IBAN.",
        ),
        commented(
            "creditor_acct_other",
            DataType::Utf8,
            "CdtTrfTxInf/CdtrAcct/Id/Othr/Id.",
        ),
        commented(
            "creditor_agent_bic",
            DataType::Utf8,
            "CdtTrfTxInf/CdtrAgt/FinInstnId/BICFI.",
        ),
        commented(
            "intermediary_agent1_bic",
            DataType::Utf8,
            "CdtTrfTxInf/IntrmyAgt1/FinInstnId/BICFI.",
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

/// Parse one file's pacs.008 transactions into a batch.
pub fn build(path: &str, content: &str) -> arrow_array::RecordBatch {
    let rows = core::parse(content);
    let s = schema();
    let cols = vec![
        str_col(rows.iter().map(|r| r.msg_id.clone())),
        ts_col(rows.iter().map(|r| r.creation_dt)),
        int_col(rows.iter().map(|r| r.nb_of_txs)),
        str_col(rows.iter().map(|r| r.settlement_method.clone())),
        str_col(rows.iter().map(|r| r.instr_id.clone())),
        str_col(rows.iter().map(|r| r.end_to_end_id.clone())),
        str_col(rows.iter().map(|r| r.tx_id.clone())),
        str_col(rows.iter().map(|r| r.uetr.clone())),
        dec_col(rows.iter().map(|r| r.amount)),
        str_col(rows.iter().map(|r| r.ccy.clone())),
        date_col(rows.iter().map(|r| r.settlement_date)),
        dec_col(rows.iter().map(|r| r.instructed_amount)),
        str_col(rows.iter().map(|r| r.instructed_ccy.clone())),
        dec_col(rows.iter().map(|r| r.exchange_rate)),
        str_col(rows.iter().map(|r| r.charge_bearer.clone())),
        str_col(rows.iter().map(|r| r.debtor_name.clone())),
        str_col(rows.iter().map(|r| r.debtor_iban.clone())),
        str_col(rows.iter().map(|r| r.debtor_acct_other.clone())),
        str_col(rows.iter().map(|r| r.debtor_agent_bic.clone())),
        str_col(rows.iter().map(|r| r.creditor_name.clone())),
        str_col(rows.iter().map(|r| r.creditor_iban.clone())),
        str_col(rows.iter().map(|r| r.creditor_acct_other.clone())),
        str_col(rows.iter().map(|r| r.creditor_agent_bic.clone())),
        str_col(rows.iter().map(|r| r.intermediary_agent1_bic.clone())),
        str_col(rows.iter().map(|r| r.purpose_code.clone())),
        list_str_col(rows.iter().map(|r| r.remittance_unstructured.clone())),
        str_col(rows.iter().map(|r| r.remittance_struct_ref.clone())),
        str_col(rows.iter().map(|_| Some(content))),
        str_col(rows.iter().map(|_| Some(path))),
    ];
    finish(&s, cols)
}

/// The `pacs008_read` table-function descriptor.
pub fn table() -> ReadTable {
    ReadTable {
        name: "pacs008_read",
        schema,
        build,
        title: "Read pacs.008 Credit Transfers",
        doc_llm:
            "Scan a glob of ISO 20022 pacs.008 (FIToFICstmrCdtTrf) XML files into one row per \
                  CdtTrfTxInf, with debtor/creditor names, IBANs, BICs, the exact settled amount \
                  and currency, end-to-end id, UETR, charge bearer, purpose, and remittance info. \
                  Use it for payment reconciliation and MT103<->pacs.008 migration-QA joins.",
        doc_md: "Read pacs.008 customer credit transfers into rows (one per CdtTrfTxInf).",
        keywords: "pacs.008, pacs008, FIToFICstmrCdtTrf, credit transfer, payments, iso 20022, \
                   end to end id, uetr, iban, bic, settled amount, reconciliation, cbpr+",
        result_columns_md: RESULT_MD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::cast::AsArray;
    use arrow_array::types::Decimal128Type;

    #[test]
    fn build_one_tx() {
        let xml = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>M</MsgId></GrpHdr><CdtTrfTxInf><PmtId><EndToEndId>E2E</EndToEndId></PmtId><IntrBkSttlmAmt Ccy="EUR">1234.56</IntrBkSttlmAmt><Dbtr><Nm>ACME</Nm></Dbtr></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;
        let b = build("/x.xml", xml);
        assert_eq!(b.num_rows(), 1);
        assert_eq!(b.schema(), schema());
        let amt = b
            .column_by_name("amount")
            .unwrap()
            .as_primitive::<Decimal128Type>();
        assert_eq!(amt.value(0), 1_234_560_000_000);
    }
}
