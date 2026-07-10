//! `camt054_read(glob)` — one row per `Ntfctn`. Reuses the camt.053 header
//! schema (identical columns); only the parse entry point differs.

use iso20022_core::mx::camt054 as core;

use super::camt053::{build_rows, schema};
use super::common::ReadTable;

const EXAMPLES: &str = r#"[{"description":"Parse an inline camt.054 debit/credit notification header: account, currency, and how many advice entries it carries.","sql":"SELECT msg_id, account_iban, account_ccy, entry_count FROM iso20022.main.camt054_read('<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.054.001.08\"><BkToCstmrDbtCdtNtfctn><GrpHdr><MsgId>CAMT054-1</MsgId><CreDtTm>2026-01-03T06:30:00Z</CreDtTm></GrpHdr><Ntfctn><Id>NTF-1</Id><Acct><Id><IBAN>DE89370400440532013000</IBAN></Id><Ccy>EUR</Ccy></Acct><Ntry><Amt Ccy=\"EUR\">250.50</Amt><CdtDbtInd>DBIT</CdtDbtInd><Sts><Cd>BOOK</Cd></Sts><NtryDtls><TxDtls><Refs><EndToEndId>E2E-DEBIT-9</EndToEndId></Refs></TxDtls></NtryDtls></Ntry></Ntfctn></BkToCstmrDbtCdtNtfctn></Document>') WHERE account_iban IS NOT NULL"}]"#;

/// Parse one file's camt.054 notifications into a batch.
pub fn build(path: &str, content: &str) -> arrow_array::RecordBatch {
    let rows = core::parse(content);
    build_rows(path, content, &rows)
}

/// The `camt054_read` table-function descriptor.
pub fn table() -> ReadTable {
    ReadTable {
        name: "camt054_read",
        schema,
        build,
        title: "Read camt.054 Debit/Credit Notifications",
        doc_llm:
            "Scan a glob of ISO 20022 camt.054 (BkToCstmrDbtCdtNtfctn) XML files into one row \
                  per notification, sharing the camt.053 statement column layout (account, period, \
                  balances when present, entry count). Pair with camt054_entries(raw) to explode \
                  the individual Ntry advice lines — identical in shape to camt053_entries.",
        doc_md: "Read camt.054 debit/credit notifications into rows (one per Ntfctn).",
        keywords: "camt.054, camt054, BkToCstmrDbtCdtNtfctn, notification, debit credit advice, \
                   iso 20022, entries, reconciliation",
        executable_examples: EXAMPLES,
    }
}
