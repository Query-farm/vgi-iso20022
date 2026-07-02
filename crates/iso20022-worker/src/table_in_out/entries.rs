//! `camt053_entries(input)` / `camt054_entries(input)` — explode camt statement /
//! notification `Ntry` rows (identical column shape). The input relation's `raw`
//! column (or the column named by `msg :=`) is parsed per row; every input column
//! is passed through so entries correlate back to their statement.

use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_schema::{Schema, SchemaRef};
use iso20022_core::mx::camt::{self, Entry};

use super::common::PerMessageTable;
use crate::cols::*;

const RESULT_MD: &str = "The passthrough input columns, then one row per `Ntry` (multi-`TxDtls` \
split by `tx_idx`): `entry_idx`, `tx_idx`, `amount` DECIMAL(38,9) + `ccy`, `credit_debit` \
(CRDT/DBIT), `reversal`, `status`, `booking_date`, `value_date`, `account_servicer_ref`, the four \
`bank_tx_*` codes, the `end_to_end_id`/`tx_id`/`uetr`/`instr_id`/`mandate_id` references, \
`counterparty_name`/`_iban`/`_agent_bic` (debtor for CRDT, creditor for DBIT), \
`remittance_unstructured` VARCHAR[], `remittance_struct_ref`, and `addtl_entry_info`.";

const EXAMPLES: &str = r#"[{"description":"Explode the entries of an inline camt.053 statement.","sql":"SELECT amount, credit_debit, end_to_end_id FROM iso20022.main.camt053_entries((SELECT '<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.08\"><BkToCstmrStmt><Stmt><Id>S</Id><Ntry><Amt Ccy=\"EUR\">500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd><NtryDtls><TxDtls><Refs><EndToEndId>E2E</EndToEndId></Refs></TxDtls></NtryDtls></Ntry></Stmt></BkToCstmrStmt></Document>' AS raw))"}]"#;

const EXAMPLES_054: &str = r#"[{"description":"Explode the entries of an inline camt.054 notification.","sql":"SELECT amount, credit_debit, end_to_end_id FROM iso20022.main.camt054_entries((SELECT '<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.054.001.08\"><BkToCstmrDbtCdtNtfctn><Ntfctn><Id>N</Id><Ntry><Amt Ccy=\"EUR\">250.50</Amt><CdtDbtInd>DBIT</CdtDbtInd><NtryDtls><TxDtls><Refs><EndToEndId>E2E</EndToEndId></Refs></TxDtls></NtryDtls></Ntry></Ntfctn></BkToCstmrDbtCdtNtfctn></Document>' AS raw))"}]"#;

/// The child-only `Ntry` output schema (appended after the passthrough columns).
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented(
            "entry_idx",
            DataType::Int32,
            "Ordinal of the Ntry within the statement.",
        ),
        commented(
            "tx_idx",
            DataType::Int32,
            "Ordinal of the TxDtls within the entry.",
        ),
        commented("amount", money_type(), "Ntry/Amt."),
        commented("ccy", DataType::Utf8, "Ntry/Amt/@Ccy."),
        commented(
            "credit_debit",
            DataType::Utf8,
            "Ntry/CdtDbtInd (CRDT/DBIT).",
        ),
        commented("reversal", DataType::Boolean, "Ntry/RvslInd."),
        commented("status", DataType::Utf8, "Ntry/Sts (BOOK/PDNG/INFO)."),
        commented(
            "booking_date",
            DataType::Date32,
            "Ntry/BookgDt/Dt (or DtTm).",
        ),
        commented("value_date", DataType::Date32, "Ntry/ValDt/Dt."),
        commented("account_servicer_ref", DataType::Utf8, "Ntry/AcctSvcrRef."),
        commented("bank_tx_domain", DataType::Utf8, "Ntry/BkTxCd/Domn/Cd."),
        commented(
            "bank_tx_family",
            DataType::Utf8,
            "Ntry/BkTxCd/Domn/Fmly/Cd.",
        ),
        commented(
            "bank_tx_subfamily",
            DataType::Utf8,
            "Ntry/BkTxCd/Domn/Fmly/SubFmlyCd.",
        ),
        commented(
            "bank_tx_proprietary",
            DataType::Utf8,
            "Ntry/BkTxCd/Prtry/Cd.",
        ),
        commented("end_to_end_id", DataType::Utf8, "TxDtls/Refs/EndToEndId."),
        commented("tx_id", DataType::Utf8, "TxDtls/Refs/TxId."),
        commented("uetr", DataType::Utf8, "TxDtls/Refs/UETR."),
        commented("instr_id", DataType::Utf8, "TxDtls/Refs/InstrId."),
        commented("mandate_id", DataType::Utf8, "TxDtls/Refs/MndtId."),
        commented(
            "counterparty_name",
            DataType::Utf8,
            "Debtor (CRDT) / creditor (DBIT) name.",
        ),
        commented(
            "counterparty_iban",
            DataType::Utf8,
            "Counterparty account IBAN.",
        ),
        commented(
            "counterparty_agent_bic",
            DataType::Utf8,
            "Counterparty agent BICFI.",
        ),
        commented(
            "remittance_unstructured",
            list_utf8_type(),
            "TxDtls/RmtInf/Ustrd (repeatable).",
        ),
        commented(
            "remittance_struct_ref",
            DataType::Utf8,
            "TxDtls/RmtInf/Strd/CdtrRefInf/Ref.",
        ),
        commented("addtl_entry_info", DataType::Utf8, "Ntry/AddtlNtryInf."),
    ]))
}

/// Build child columns + per-message child counts from the input messages.
pub fn build(messages: &[String]) -> (Vec<usize>, Vec<ArrayRef>) {
    let mut all: Vec<Entry> = Vec::new();
    let mut counts = Vec::with_capacity(messages.len());
    for m in messages {
        let e = camt::parse_entries(m);
        counts.push(e.len());
        all.extend(e);
    }
    (counts, columns(&all))
}

fn columns(rows: &[Entry]) -> Vec<ArrayRef> {
    vec![
        int_col(rows.iter().map(|r| Some(r.entry_idx))),
        int_col(rows.iter().map(|r| Some(r.tx_idx))),
        dec_col(rows.iter().map(|r| r.amount)),
        str_col(rows.iter().map(|r| r.ccy.clone())),
        str_col(rows.iter().map(|r| r.credit_debit.clone())),
        bool_col(rows.iter().map(|r| r.reversal)),
        str_col(rows.iter().map(|r| r.status.clone())),
        date_col(rows.iter().map(|r| r.booking_date)),
        date_col(rows.iter().map(|r| r.value_date)),
        str_col(rows.iter().map(|r| r.account_servicer_ref.clone())),
        str_col(rows.iter().map(|r| r.bank_tx_domain.clone())),
        str_col(rows.iter().map(|r| r.bank_tx_family.clone())),
        str_col(rows.iter().map(|r| r.bank_tx_subfamily.clone())),
        str_col(rows.iter().map(|r| r.bank_tx_proprietary.clone())),
        str_col(rows.iter().map(|r| r.end_to_end_id.clone())),
        str_col(rows.iter().map(|r| r.tx_id.clone())),
        str_col(rows.iter().map(|r| r.uetr.clone())),
        str_col(rows.iter().map(|r| r.instr_id.clone())),
        str_col(rows.iter().map(|r| r.mandate_id.clone())),
        str_col(rows.iter().map(|r| r.counterparty_name.clone())),
        str_col(rows.iter().map(|r| r.counterparty_iban.clone())),
        str_col(rows.iter().map(|r| r.counterparty_agent_bic.clone())),
        list_str_col(rows.iter().map(|r| r.remittance_unstructured.clone())),
        str_col(rows.iter().map(|r| r.remittance_struct_ref.clone())),
        str_col(rows.iter().map(|r| r.addtl_entry_info.clone())),
    ]
}

/// The `camt053_entries` descriptor.
pub fn camt053_entries() -> PerMessageTable {
    PerMessageTable {
        name: "camt053_entries",
        child_schema: schema,
        build,
        title: "Explode camt.053 Entries",
        doc_llm: "Explode the Ntry rows of camt.053 statements (or any camt.053/054 documents) into \
                  one row per entry, with multi-TxDtls split by tx_idx and every input column passed \
                  through. Pass a relation whose `raw` column (or the column named by `msg`) holds \
                  each statement — typically the output of camt053_read — to get amount, \
                  credit/debit, dates, references, counterparty, and remittance for reconciliation. \
                  See the executable example for the exact call shape.",
        doc_md: "Explode camt.053 statement Ntry rows (one per entry; multi-TxDtls split; input columns passthrough).",
        keywords: "camt.053, entries, ntry, statement lines, reconciliation, credit debit, \
                   end to end id, counterparty, passthrough, iso 20022",
        result_columns_md: RESULT_MD,
        executable_examples: EXAMPLES,
    }
}

/// The `camt054_entries` descriptor (identical Ntry shape; auto-detects parent).
pub fn camt054_entries() -> PerMessageTable {
    PerMessageTable {
        name: "camt054_entries",
        child_schema: schema,
        build,
        title: "Explode camt.054 Entries",
        doc_llm: "Explode the Ntry rows of camt.054 debit/credit notifications into one row per \
                  entry — identical column shape to camt053_entries, so reconciliation queries are \
                  portable. Pass a relation whose `raw` column (or the column named by `msg`) holds \
                  each notification, typically the output of camt054_read. See the executable \
                  example for the exact call shape.",
        doc_md: "Explode camt.054 notification Ntry rows (one per entry; multi-TxDtls split; input columns passthrough).",
        keywords: "camt.054, entries, ntry, notification, debit credit advice, reconciliation, \
                   counterparty, passthrough, iso 20022",
        result_columns_md: RESULT_MD,
        executable_examples: EXAMPLES_054,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_io::test_support::process_params;
    use arrow_array::cast::AsArray;
    use arrow_array::{Array, RecordBatch, StringArray};
    use arrow_schema::Field;
    use vgi::arguments::Arguments;
    use vgi::table_in_out::TableInOutFunction;
    use vgi::BindParams;

    const XML: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08"><BkToCstmrStmt><Stmt><Id>S</Id><Ntry><Amt Ccy="EUR">500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd><NtryDtls><TxDtls><Refs><EndToEndId>E2E</EndToEndId></Refs></TxDtls></NtryDtls></Ntry></Stmt></BkToCstmrStmt></Document>"#;

    /// A 2-column input relation: a passthrough `acct` and the `raw` message.
    fn input() -> (RecordBatch, SchemaRef) {
        let acct: ArrayRef = Arc::new(StringArray::from(vec!["DE123"]));
        let raw: ArrayRef = Arc::new(StringArray::from(vec![XML]));
        let schema = Arc::new(Schema::new(vec![
            Field::new("acct", DataType::Utf8, true),
            Field::new("raw", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(schema.clone(), vec![acct, raw]).unwrap();
        (batch, schema)
    }

    #[test]
    fn passes_input_through_and_explodes() {
        let f = camt053_entries();
        let (batch, in_schema) = input();
        let bound = f
            .on_bind(&BindParams {
                input_schema: Some(in_schema),
                arguments: Arguments::default(),
                ..Default::default()
            })
            .unwrap();
        // Output starts with the passthrough columns.
        assert_eq!(bound.output_schema.field(0).name(), "acct");
        assert_eq!(bound.output_schema.field(2).name(), "entry_idx");

        let params = process_params(bound.output_schema, Arguments::default());
        let out = f.process(&params, &batch).unwrap();
        assert_eq!(out.len(), 1);
        let b = &out[0];
        assert_eq!(b.num_rows(), 1);
        // Passthrough acct carried onto the entry row.
        assert_eq!(
            b.column_by_name("acct")
                .unwrap()
                .as_string::<i32>()
                .value(0),
            "DE123"
        );
        assert_eq!(
            b.column_by_name("credit_debit")
                .unwrap()
                .as_string::<i32>()
                .value(0),
            "CRDT"
        );
        assert!(!b.column_by_name("end_to_end_id").unwrap().is_null(0));
    }
}
