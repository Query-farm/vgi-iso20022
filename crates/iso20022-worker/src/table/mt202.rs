//! `mt202_read(glob)` — one row per MT202 / MT202 COV message.

use std::sync::Arc;

use arrow_schema::{Schema, SchemaRef};
use iso20022_core::mt::block;
use iso20022_core::mt::mt202::{self, Mt202};

use super::common::ReadTable;
use super::scan::finish;
use crate::cols::*;

const EXAMPLES: &str = r#"[{"description":"Parse an inline MT202 (general financial institution transfer / cover) and read its settled amount and beneficiary institution.","sql":"SELECT transaction_ref, related_ref, ccy, amount, beneficiary_inst FROM iso20022.main.mt202_read('{1:F01ACMEDEFFAXXX0000000000}{2:I202DEUTDEFFXXXXN}{3:{121:11111111-2222-4333-8444-555566667777}}{4:\n:20:FI-REF-9\n:21:REL-REF-9\n:32A:260102USD5000000,00\n:52A:CHASUS33\n:58A:DEUTDEFF\n:50K:/111\nUNDERLYING DEBTOR\n:59:/222\nUNDERLYING CREDITOR\n:70:COVER FOR MT103\n-}') WHERE amount > 1000000"}]"#;

/// The fixed output schema (column order matches [`build`]).
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented("transaction_ref", DataType::Utf8, ":20:."),
        commented("related_ref", DataType::Utf8, ":21:."),
        commented("value_date", DataType::Date32, ":32A: date."),
        commented("ccy", DataType::Utf8, ":32A: currency."),
        commented("amount", money_type(), ":32A: amount."),
        commented("ordering_institution", DataType::Utf8, ":52A/D:."),
        commented("senders_correspondent", DataType::Utf8, ":53A/B/D:."),
        commented("receivers_correspondent", DataType::Utf8, ":54A/B/D:."),
        commented("intermediary", DataType::Utf8, ":56A/D:."),
        commented("account_with_inst", DataType::Utf8, ":57A/B/D:."),
        commented("beneficiary_inst", DataType::Utf8, ":58A/D:."),
        commented("sender_to_receiver_info", DataType::Utf8, ":72:."),
        commented(
            "is_cover",
            DataType::Boolean,
            "True when an underlying customer (COV) block is present.",
        ),
        commented("cov_ordering_customer", DataType::Utf8, "COV :50A/F/K:."),
        commented("cov_beneficiary", DataType::Utf8, "COV :59/59A/59F:."),
        commented("cov_remittance_info", DataType::Utf8, "COV :70:."),
        commented("uetr", DataType::Utf8, "Block-3 {121:} UETR."),
        commented("raw", DataType::Utf8, "The whole message."),
        commented("path", DataType::Utf8, "Source file path."),
    ]))
}

/// Parse one file (one MT202 message) into a batch; zero rows if no `:20:`.
pub fn build(path: &str, content: &str) -> arrow_array::RecordBatch {
    let rows: Vec<Mt202> = if block::parse(content).has("20") {
        vec![mt202::parse(content)]
    } else {
        Vec::new()
    };
    let s = schema();
    let cols = vec![
        str_col(rows.iter().map(|r| r.transaction_ref.clone())),
        str_col(rows.iter().map(|r| r.related_ref.clone())),
        date_col(rows.iter().map(|r| r.value_date)),
        str_col(rows.iter().map(|r| r.ccy.clone())),
        dec_col(rows.iter().map(|r| r.amount)),
        str_col(rows.iter().map(|r| r.ordering_institution.clone())),
        str_col(rows.iter().map(|r| r.senders_correspondent.clone())),
        str_col(rows.iter().map(|r| r.receivers_correspondent.clone())),
        str_col(rows.iter().map(|r| r.intermediary.clone())),
        str_col(rows.iter().map(|r| r.account_with_inst.clone())),
        str_col(rows.iter().map(|r| r.beneficiary_inst.clone())),
        str_col(rows.iter().map(|r| r.sender_to_receiver_info.clone())),
        bool_col(rows.iter().map(|r| Some(r.is_cover))),
        str_col(rows.iter().map(|r| r.cov_ordering_customer.clone())),
        str_col(rows.iter().map(|r| r.cov_beneficiary.clone())),
        str_col(rows.iter().map(|r| r.cov_remittance_info.clone())),
        str_col(rows.iter().map(|r| r.uetr.clone())),
        str_col(rows.iter().map(|_| Some(content))),
        str_col(rows.iter().map(|_| Some(path))),
    ];
    finish(&s, cols)
}

/// The `mt202_read` table-function descriptor.
pub fn table() -> ReadTable {
    ReadTable {
        name: "mt202_read",
        schema,
        build,
        title: "Read MT202 / MT202 COV FI Transfers",
        doc_llm: "Scan a glob of SWIFT MT202 / MT202 COV (general financial-institution transfer) \
                  text files into one row per message: references, value date, amount and currency, \
                  the institution chain, and — for cover messages — the underlying customer block \
                  (is_cover, cov_ordering_customer, cov_beneficiary, cov_remittance_info).",
        doc_md: "Read SWIFT MT202 / MT202 COV financial-institution transfers into rows.",
        keywords: "mt202, mt202 cov, swift mt, fi transfer, cover payment, correspondent banking, \
                   beneficiary institution, uetr, fin",
        executable_examples: EXAMPLES,
    }
}
