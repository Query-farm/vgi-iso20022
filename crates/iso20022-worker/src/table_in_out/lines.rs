//! `mt940_lines(input)` / `mt942_lines(input)` — explode MT940/MT942 `:61:`
//! statement lines (each joined to its `:86:` narrative). Identical column shape.
//! The input relation's `raw` column (or the `msg :=` column) is parsed per row;
//! every input column is passed through so lines correlate back to the statement.

use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_schema::{Schema, SchemaRef};
use iso20022_core::mt::mt940::{self, Line};

use super::common::PerMessageTable;
use crate::cols::*;

const EXAMPLES: &str = r#"[{"description":"Read an MT940 statement inline, then explode its :61: lines (one row per line, joined to its :86: narrative).","sql":"SELECT value_date, credit_debit, amount, transaction_type_id FROM iso20022.main.mt940_lines((SELECT raw FROM iso20022.main.mt940_read('{1:F01ACMEDEFFAXXX0000000000}{2:O940DEUTDEFFXXXXN}{4:\n:20:STMT-1\n:25:DE89370400440532013000\n:28C:12345/1\n:60F:C260101EUR1000,00\n:61:2601020102C500,00NTRFNONREF//BANK-A\n:86:GROCERY STORE PAYMENT\n:61:2601030103D250,50NMSCCUST-REF//BANK-B\n:86:RENT\n:62F:C260103EUR1249,50\n-}')))"}]"#;
const EXAMPLES_942: &str = r#"[{"description":"Read an MT942 interim report inline, then explode its :61: lines.","sql":"SELECT value_date, credit_debit, amount FROM iso20022.main.mt942_lines((SELECT raw FROM iso20022.main.mt942_read('{1:F01ACMEDEFFAXXX0000000000}{2:O942DEUTDEFFXXXXN}{4:\n:20:INTERIM-1\n:25:DE89370400440532013000\n:28C:99/1\n:34F:EURD0,00\n:34F:EURC1000,00\n:61:2601020102C500,00NTRFNONREF//BANK-A\n:86:INCOMING\n-}')))"}]"#;

/// The child-only `:61:`/`:86:` line schema (appended after the passthrough columns).
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented("line_idx", DataType::Int32, "Ordinal within the statement."),
        commented("value_date", DataType::Date32, ":61: subfield 1 (YYMMDD)."),
        commented(
            "entry_date",
            DataType::Date32,
            ":61: subfield 2 (MMDD; year from value_date).",
        ),
        commented(
            "credit_debit",
            DataType::Utf8,
            ":61: subfield 3 mark: C/D/RC/RD.",
        ),
        commented("funds_code", DataType::Utf8, ":61: subfield 3 funds code."),
        commented(
            "amount",
            money_type(),
            ":61: subfield 4 (positive; sign via credit_debit).",
        ),
        commented(
            "transaction_type_id",
            DataType::Utf8,
            ":61: subfield 5 (e.g. NTRF, NMSC).",
        ),
        commented("customer_ref", DataType::Utf8, ":61: subfield 6 up to //."),
        commented("bank_ref", DataType::Utf8, ":61: subfield 6 after //."),
        commented("supplementary", DataType::Utf8, ":61: subfield 7."),
        commented(
            "narrative",
            DataType::Utf8,
            ":86: flattened (continuations joined).",
        ),
        commented(
            "narrative_struct",
            map_utf8_type(),
            ":86: structured ?NN/>NN subfields.",
        ),
    ]))
}

/// Build child columns + per-message child counts from the input messages.
pub fn build(messages: &[String]) -> (Vec<usize>, Vec<ArrayRef>) {
    let mut all: Vec<Line> = Vec::new();
    let mut counts = Vec::with_capacity(messages.len());
    for m in messages {
        let l = mt940::parse_lines(m);
        counts.push(l.len());
        all.extend(l);
    }
    (counts, columns(&all))
}

fn columns(rows: &[Line]) -> Vec<ArrayRef> {
    vec![
        int_col(rows.iter().map(|r| Some(r.line_idx))),
        date_col(rows.iter().map(|r| r.value_date)),
        date_col(rows.iter().map(|r| r.entry_date)),
        str_col(rows.iter().map(|r| r.credit_debit.clone())),
        str_col(rows.iter().map(|r| r.funds_code.clone())),
        dec_col(rows.iter().map(|r| r.amount)),
        str_col(rows.iter().map(|r| r.transaction_type_id.clone())),
        str_col(rows.iter().map(|r| r.customer_ref.clone())),
        str_col(rows.iter().map(|r| r.bank_ref.clone())),
        str_col(rows.iter().map(|r| r.supplementary.clone())),
        str_col(rows.iter().map(|r| r.narrative.clone())),
        map_str_col(rows.iter().map(|r| r.narrative_struct.clone())),
    ]
}

/// The `mt940_lines` descriptor.
pub fn mt940_lines() -> PerMessageTable {
    PerMessageTable {
        name: "mt940_lines",
        child_schema: schema,
        build,
        title: "Explode MT940 Statement Lines",
        doc_llm: "Explode the :61: statement lines of one or more MT940 end-of-day statements — \
                  each :61: line joined to its following :86: narrative — into one row per line. \
                  Pass a relation whose `raw` column (or the column named by `msg :=`) holds each \
                  statement: for example the output of `mt940_read('…')`, or any relation exposing \
                  a `raw` message column, or an inline statement. Every input column is passed \
                  through, repeated once per child line, so lines correlate back to their parent \
                  statement. Each output line carries `line_idx`, `value_date` and `entry_date` \
                  (`DATE`), the `credit_debit` mark (C/D/RC/RD), positive `amount` `DECIMAL(38,9)`, \
                  `transaction_type_id` (e.g. NTRF/NMSC), `customer_ref`/`bank_ref`, the flattened \
                  `narrative`, and `narrative_struct` `MAP(VARCHAR, VARCHAR)` for structured \
                  `?NN`/`>NN` narratives.",
        doc_md: "Explode MT940 :61: statement lines (one per line, joined to :86:; input columns passthrough).",
        keywords: "mt940, statement lines, 61, 86, narrative, credit debit, transaction type, \
                   reconciliation, passthrough, fin",
        executable_examples: EXAMPLES,
    }
}

/// The `mt942_lines` descriptor (identical line shape).
pub fn mt942_lines() -> PerMessageTable {
    PerMessageTable {
        name: "mt942_lines",
        child_schema: schema,
        build,
        title: "Explode MT942 Report Lines",
        doc_llm: "Explode the :61: lines of one or more MT942 interim (intra-day) reports — each \
                  :61: line joined to its following :86: narrative — into one row per line. \
                  Identical column shape to mt940_lines (`line_idx`, `value_date`/`entry_date`, \
                  `credit_debit`, positive `amount` `DECIMAL(38,9)`, `transaction_type_id`, \
                  references, flattened `narrative`, and `narrative_struct` `MAP`), with every input \
                  column passed through so lines correlate back to their parent report. Pass a \
                  relation whose `raw` column (or the column named by `msg :=`) holds each report: \
                  for example the output of `mt942_read('…')`, or an inline report.",
        doc_md: "Explode MT942 :61: report lines (one per line, joined to :86:; input columns passthrough).",
        keywords: "mt942, interim report, statement lines, 61, 86, narrative, intraday, \
                   reconciliation, passthrough, fin",
        executable_examples: EXAMPLES_942,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::process_params;
    use arrow_array::cast::AsArray;
    use arrow_array::{RecordBatch, StringArray};
    use arrow_schema::Field;
    use vgi::arguments::Arguments;
    use vgi::table_in_out::TableInOutFunction;
    use vgi::BindParams;

    #[test]
    fn explodes_lines_with_passthrough() {
        let f = mt940_lines();
        let stmt = ":20:S\n:61:2601020102C500,00NTRFNONREF//BANK-A\n:86:GROCERY\n:61:2601030103D250,50NMSCREF//BANK-B\n:86:RENT";
        let acct: ArrayRef = Arc::new(StringArray::from(vec!["DE123"]));
        let raw: ArrayRef = Arc::new(StringArray::from(vec![stmt]));
        let in_schema = Arc::new(Schema::new(vec![
            Field::new("account", DataType::Utf8, true),
            Field::new("raw", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(in_schema.clone(), vec![acct, raw]).unwrap();
        let bound = f
            .on_bind(&BindParams {
                input_schema: Some(in_schema),
                arguments: Arguments::default(),
                ..Default::default()
            })
            .unwrap();
        let params = process_params(bound.output_schema, Arguments::default());
        let out = f.process(&params, &batch).unwrap();
        assert_eq!(out[0].num_rows(), 2);
        let acct = out[0].column_by_name("account").unwrap().as_string::<i32>();
        assert_eq!(acct.value(0), "DE123");
        assert_eq!(acct.value(1), "DE123");
    }
}
