//! `mt940_read(glob)` — one row per `:20:`/`:28C:` statement (a file may hold
//! several; `statement_idx` disambiguates within a file).

use std::sync::Arc;

use arrow_schema::{Schema, SchemaRef};
use iso20022_core::mt::mt940;

use super::common::ReadTable;
use super::scan::finish;
use crate::cols::*;

const RESULT_MD: &str =
    "One row per statement (`statement_idx` orders multiple statements within a \
file): `transaction_ref`, `related_ref`, `account`, `statement_no`/`sequence_no`, signed \
`opening_balance`/`closing_balance` + their D/C marks and dates, `opening_is_intermediate`, `ccy`, \
`closing_available`, `forward_available`, `line_count`, plus `raw` (this statement's text, for \
`mt940_lines(raw)`) and `path`.";

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
            "opening_balance",
            money_type(),
            ":60F/M: amount (signed by D/C).",
        ),
        commented("opening_balance_dc", DataType::Utf8, ":60F/M: D or C."),
        commented("opening_balance_date", DataType::Date32, ":60F/M: date."),
        commented(
            "opening_is_intermediate",
            DataType::Boolean,
            "True for :60M: (vs :60F:).",
        ),
        commented("ccy", DataType::Utf8, "Balance currency."),
        commented("closing_balance", money_type(), ":62F/M: amount (signed)."),
        commented("closing_balance_dc", DataType::Utf8, ":62F/M: D or C."),
        commented("closing_balance_date", DataType::Date32, ":62F/M: date."),
        commented(
            "closing_available",
            money_type(),
            ":64: closing available (signed).",
        ),
        commented(
            "forward_available",
            money_type(),
            ":65: forward available (first; signed).",
        ),
        commented("line_count", DataType::Int32, "Count of :61: lines."),
        commented("raw", DataType::Utf8, "This statement's text."),
        commented("path", DataType::Utf8, "Source file path."),
    ]))
}

/// Parse one file's statements into a batch.
pub fn build(path: &str, content: &str) -> arrow_array::RecordBatch {
    let rows = mt940::parse_file(content);
    let s = schema();
    let cols = vec![
        int_col(rows.iter().map(|r| Some(r.statement_idx))),
        str_col(rows.iter().map(|r| r.transaction_ref.clone())),
        str_col(rows.iter().map(|r| r.related_ref.clone())),
        str_col(rows.iter().map(|r| r.account.clone())),
        str_col(rows.iter().map(|r| r.statement_no.clone())),
        str_col(rows.iter().map(|r| r.sequence_no.clone())),
        dec_col(rows.iter().map(|r| r.opening_balance)),
        str_col(rows.iter().map(|r| r.opening_balance_dc.clone())),
        date_col(rows.iter().map(|r| r.opening_balance_date)),
        bool_col(rows.iter().map(|r| Some(r.opening_is_intermediate))),
        str_col(rows.iter().map(|r| r.ccy.clone())),
        dec_col(rows.iter().map(|r| r.closing_balance)),
        str_col(rows.iter().map(|r| r.closing_balance_dc.clone())),
        date_col(rows.iter().map(|r| r.closing_balance_date)),
        dec_col(rows.iter().map(|r| r.closing_available)),
        dec_col(rows.iter().map(|r| r.forward_available)),
        int_col(rows.iter().map(|r| Some(r.line_count))),
        str_col(rows.iter().map(|r| Some(r.raw.clone()))),
        str_col(rows.iter().map(|_| Some(path))),
    ];
    finish(&s, cols)
}

/// The `mt940_read` table-function descriptor.
pub fn table() -> ReadTable {
    ReadTable {
        name: "mt940_read",
        schema,
        build,
        title: "Read MT940 Statements",
        doc_llm: "Scan a glob of SWIFT MT940 (customer statement) text files into one row per \
                  statement (a file may hold several; statement_idx orders them). Columns include \
                  the account, statement/sequence numbers, signed opening and closing balances with \
                  their dates, and the line count. Use the raw column with mt940_lines(raw) to \
                  explode the :61: statement lines for reconciliation.",
        doc_md: "Read SWIFT MT940 customer statements into rows (one per statement).",
        keywords: "mt940, swift mt, bank statement, opening balance, closing balance, 60F, 62F, \
                   statement lines, reconciliation, fin",
        result_columns_md: RESULT_MD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_statements() {
        let m = "{1:F01X}{2:O940X}{4:\n:20:S1\n:25:DE89370400440532013000\n:28C:1/1\n:60F:C260101EUR1000,00\n:61:2601020102C500,00NTRFR//B\n:86:PAY\n:62F:C260102EUR1500,00\n-}";
        let b = build("/x.txt", m);
        assert_eq!(b.num_rows(), 1);
        assert_eq!(b.schema(), schema());
    }
}
