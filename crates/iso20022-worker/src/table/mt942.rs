//! `mt942_read(glob)` — one row per `:20:` interim report (a file may hold
//! several; `statement_idx` disambiguates within a file).

use std::sync::Arc;

use arrow_schema::{Schema, SchemaRef};
use iso20022_core::mt::mt942;

use super::common::ReadTable;
use super::scan::finish;
use crate::cols::*;

const RESULT_MD: &str =
    "One row per interim report (`statement_idx` orders multiple within a file): \
`transaction_ref`, `related_ref`, `account`, `statement_no`/`sequence_no`, `floor_limit_debit` / \
`floor_limit_credit` (:34F:) + `ccy`, `datetime_indication` (:13D: TIMESTAMPTZ), \
`debit_count`/`debit_sum` (:90D:), `credit_count`/`credit_sum` (:90C:), `line_count`, plus `raw` \
(for `mt942_lines(raw)`) and `path`.";

/// The fixed output schema (column order matches [`build`]).
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented(
            "statement_idx",
            DataType::Int32,
            "0-based ordinal within the file.",
        ),
        commented("transaction_ref", DataType::Utf8, ":20:."),
        commented("related_ref", DataType::Utf8, ":21:."),
        commented("account", DataType::Utf8, ":25: account id / IBAN."),
        commented("statement_no", DataType::Utf8, ":28C: statement number."),
        commented("sequence_no", DataType::Utf8, ":28C: sequence number."),
        commented(
            "floor_limit_debit",
            money_type(),
            ":34F: debit floor limit.",
        ),
        commented(
            "floor_limit_credit",
            money_type(),
            ":34F: credit floor limit.",
        ),
        commented("ccy", DataType::Utf8, "Floor-limit / summary currency."),
        commented(
            "datetime_indication",
            timestamp_type(),
            ":13D: date/time indication.",
        ),
        commented("debit_count", DataType::Int32, ":90D: debit entry count."),
        commented("debit_sum", money_type(), ":90D: debit sum."),
        commented("credit_count", DataType::Int32, ":90C: credit entry count."),
        commented("credit_sum", money_type(), ":90C: credit sum."),
        commented("line_count", DataType::Int32, "Count of :61: lines."),
        commented("raw", DataType::Utf8, "This report's text."),
        commented("path", DataType::Utf8, "Source file path."),
    ]))
}

/// Parse one file's interim reports into a batch.
pub fn build(path: &str, content: &str) -> arrow_array::RecordBatch {
    let rows = mt942::parse_file(content);
    let s = schema();
    let cols = vec![
        int_col(rows.iter().map(|r| Some(r.statement_idx))),
        str_col(rows.iter().map(|r| r.transaction_ref.clone())),
        str_col(rows.iter().map(|r| r.related_ref.clone())),
        str_col(rows.iter().map(|r| r.account.clone())),
        str_col(rows.iter().map(|r| r.statement_no.clone())),
        str_col(rows.iter().map(|r| r.sequence_no.clone())),
        dec_col(rows.iter().map(|r| r.floor_limit_debit)),
        dec_col(rows.iter().map(|r| r.floor_limit_credit)),
        str_col(rows.iter().map(|r| r.ccy.clone())),
        ts_col(rows.iter().map(|r| r.datetime_indication)),
        int_col(rows.iter().map(|r| r.debit_count)),
        dec_col(rows.iter().map(|r| r.debit_sum)),
        int_col(rows.iter().map(|r| r.credit_count)),
        dec_col(rows.iter().map(|r| r.credit_sum)),
        int_col(rows.iter().map(|r| Some(r.line_count))),
        str_col(rows.iter().map(|r| Some(r.raw.clone()))),
        str_col(rows.iter().map(|_| Some(path))),
    ];
    finish(&s, cols)
}

/// The `mt942_read` table-function descriptor.
pub fn table() -> ReadTable {
    ReadTable {
        name: "mt942_read",
        schema,
        build,
        title: "Read MT942 Interim Reports",
        doc_llm: "Scan a glob of SWIFT MT942 (interim transaction report) text files into one row \
                  per report: floor limits, the datetime indication, debit/credit count and sum \
                  summaries, and the line count. Use the raw column with mt942_lines(raw) to explode \
                  the :61: lines — identical in shape to mt940_lines.",
        doc_md: "Read SWIFT MT942 interim transaction reports into rows (one per report).",
        keywords: "mt942, swift mt, interim report, floor limit, 34F, 90D, 90C, intraday, \
                   statement lines, fin",
        result_columns_md: RESULT_MD,
    }
}
