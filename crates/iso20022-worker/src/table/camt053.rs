//! `camt053_read(glob)` — one row per `Stmt`. The camt.054 reader reuses this
//! schema (the `Ntfctn` header carries the same columns).

use std::sync::Arc;

use arrow_schema::{Schema, SchemaRef};
use iso20022_core::mx::camt053::Statement;

use super::common::ReadTable;
use super::scan::finish;
use crate::cols::*;

const EXAMPLES: &str = r#"[{"description":"Parse an inline camt.053 bank-to-customer statement header: account, closing balance, and how many booked entries it carries.","sql":"SELECT msg_id, account_iban, account_ccy, closing_balance, entry_count FROM iso20022.main.camt053_read('<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.08\"><BkToCstmrStmt><GrpHdr><MsgId>CAMT053-1</MsgId><CreDtTm>2026-01-03T06:00:00Z</CreDtTm></GrpHdr><Stmt><Id>STMT-1</Id><ElctrncSeqNb>5</ElctrncSeqNb><Acct><Id><IBAN>DE89370400440532013000</IBAN></Id><Ccy>EUR</Ccy><Ownr><Nm>ACME CORP</Nm></Ownr></Acct><Bal><Tp><CdOrPrtry><Cd>CLBD</Cd></CdOrPrtry></Tp><Amt Ccy=\"EUR\">1500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd></Bal><Ntry><Amt Ccy=\"EUR\">500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd><Sts><Cd>BOOK</Cd></Sts><BookgDt><Dt>2026-01-02</Dt></BookgDt><ValDt><Dt>2026-01-02</Dt></ValDt><NtryDtls><TxDtls><Refs><EndToEndId>E2E-REF-001</EndToEndId></Refs></TxDtls></NtryDtls></Ntry></Stmt></BkToCstmrStmt></Document>') WHERE account_iban IS NOT NULL"}]"#;

/// The fixed output schema (shared with the camt.054 notification reader).
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented("msg_id", DataType::Utf8, "GrpHdr/MsgId."),
        commented("creation_dt", timestamp_type(), "GrpHdr/CreDtTm."),
        commented(
            "stmt_id",
            DataType::Utf8,
            "Stmt/Id (or Ntfctn/Id for camt.054).",
        ),
        commented(
            "stmt_seq_nb",
            DataType::Utf8,
            "Stmt/ElctrncSeqNb or LglSeqNb.",
        ),
        commented("account_iban", DataType::Utf8, "Acct/Id/IBAN."),
        commented("account_other", DataType::Utf8, "Acct/Id/Othr/Id."),
        commented("account_ccy", DataType::Utf8, "Acct/Ccy."),
        commented("account_owner", DataType::Utf8, "Acct/Ownr/Nm."),
        commented("from_dt", timestamp_type(), "FrToDt/FrDtTm."),
        commented("to_dt", timestamp_type(), "FrToDt/ToDtTm."),
        commented(
            "opening_balance",
            money_type(),
            "OPBD balance (signed by CdtDbtInd).",
        ),
        commented("closing_balance", money_type(), "CLBD balance (signed)."),
        commented("closing_available", money_type(), "CLAV balance (signed)."),
        commented("ccy", DataType::Utf8, "Balance currency (@Ccy)."),
        commented("entry_count", DataType::Int32, "Count of Ntry."),
        commented("sum_credits", money_type(), "TxsSummry/TtlCdtNtries/Sum."),
        commented("sum_debits", money_type(), "TxsSummry/TtlDbtNtries/Sum."),
        commented("raw", DataType::Utf8, "The whole source document."),
        commented("path", DataType::Utf8, "Source file path."),
    ]))
}

/// Build a batch from already-parsed statement/notification headers.
pub fn build_rows(path: &str, content: &str, rows: &[Statement]) -> arrow_array::RecordBatch {
    let s = schema();
    let cols = vec![
        str_col(rows.iter().map(|r| r.msg_id.clone())),
        ts_col(rows.iter().map(|r| r.creation_dt)),
        str_col(rows.iter().map(|r| r.stmt_id.clone())),
        str_col(rows.iter().map(|r| r.stmt_seq_nb.clone())),
        str_col(rows.iter().map(|r| r.account_iban.clone())),
        str_col(rows.iter().map(|r| r.account_other.clone())),
        str_col(rows.iter().map(|r| r.account_ccy.clone())),
        str_col(rows.iter().map(|r| r.account_owner.clone())),
        ts_col(rows.iter().map(|r| r.from_dt)),
        ts_col(rows.iter().map(|r| r.to_dt)),
        dec_col(rows.iter().map(|r| r.opening_balance)),
        dec_col(rows.iter().map(|r| r.closing_balance)),
        dec_col(rows.iter().map(|r| r.closing_available)),
        str_col(rows.iter().map(|r| r.ccy.clone())),
        int_col(rows.iter().map(|r| Some(r.entry_count))),
        dec_col(rows.iter().map(|r| r.sum_credits)),
        dec_col(rows.iter().map(|r| r.sum_debits)),
        str_col(rows.iter().map(|_| Some(content))),
        str_col(rows.iter().map(|_| Some(path))),
    ];
    finish(&s, cols)
}

/// Parse one file's camt.053 statements into a batch.
pub fn build(path: &str, content: &str) -> arrow_array::RecordBatch {
    let rows = iso20022_core::mx::camt053::parse(content);
    build_rows(path, content, &rows)
}

/// The `camt053_read` table-function descriptor.
pub fn table() -> ReadTable {
    ReadTable {
        name: "camt053_read",
        schema,
        build,
        title: "Read camt.053 Statements",
        doc_llm: "Scan a glob of ISO 20022 camt.053 (BkToCstmrStmt) XML files into one row per \
                  statement: account IBAN, opening/closing/available balances (signed, exact \
                  `DECIMAL`), statement period, entry count, and credit/debit summary sums. Use the \
                  raw column with camt053_entries(raw) to explode the individual Ntry lines for \
                  reconciliation.",
        doc_md: "Read camt.053 bank-to-customer statements into rows (one per Stmt).",
        keywords: "camt.053, camt053, BkToCstmrStmt, bank statement, opening balance, closing \
                   balance, iban, reconciliation, iso 20022, statement",
        executable_examples: EXAMPLES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_one_statement() {
        let xml = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08"><BkToCstmrStmt><GrpHdr><MsgId>M</MsgId></GrpHdr><Stmt><Id>S1</Id><Acct><Id><IBAN>DE89370400440532013000</IBAN></Id></Acct><Bal><Tp><CdOrPrtry><Cd>CLBD</Cd></CdOrPrtry></Tp><Amt Ccy="EUR">1500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd></Bal><Ntry><Amt Ccy="EUR">500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd></Ntry></Stmt></BkToCstmrStmt></Document>"#;
        let b = build("/x.xml", xml);
        assert_eq!(b.num_rows(), 1);
        assert_eq!(b.schema(), schema());
    }
}
