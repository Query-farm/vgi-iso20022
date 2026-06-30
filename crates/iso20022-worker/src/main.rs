//! The `iso20022` VGI worker.
//!
//! A standalone binary that DuckDB launches and talks to over Apache Arrow IPC
//! (`ATTACH 'iso20022' (TYPE vgi, COMMAND '…')`). It parses SWIFT **MT**
//! (MT103/202/940/942) and ISO 20022 **MX** (pacs.008/pacs.002/pain.001/
//! camt.053/camt.054) payment messages into typed, queryable rows under the
//! catalog `iso20022`, schema `main`:
//!
//! ```sql
//! ATTACH 'iso20022' AS pay (TYPE vgi, COMMAND 'iso20022-worker');
//! SELECT * FROM pay.main.camt053_read('/data/statements/*.xml');
//! SELECT pay.main.iso20022_mt_type(raw) FROM read_text('/inbox/*.txt');
//! ```
//!
//! The worker makes **zero outbound calls** and registers **no secret provider** —
//! every message is parsed locally; nothing about a payment leaves the host. The
//! pure parse engine lives in the `iso20022-core` crate; the `scalar/`, `table/`,
//! and `table_in_out/` modules are thin Arrow adapters over it.

use iso20022_worker::meta::{agent_test_tasks_json, keywords_json};
use iso20022_worker::{scalar, table, table_in_out};
use vgi::catalog::{CatSchema, CatalogModel};
use vgi::Worker;

/// Catalog + schema metadata surfaced to DuckDB and the `vgi-lint` metadata
/// linter. The function objects themselves are served from the registered
/// scalars / tables / table-in-out functions; this adds catalog/schema-level
/// comments, provenance, and discovery tags.
fn catalog_metadata(name: &str) -> CatalogModel {
    CatalogModel {
        name: name.to_string(),
        comment: Some(
            "Parse SWIFT MT and ISO 20022 MX payment messages into typed, queryable rows. \
             Local-only, no network, no egress."
                .to_string(),
        ),
        tags: vec![
            ("vgi.title".to_string(), "ISO 20022 & SWIFT MT Payment Parsing".to_string()),
            (
                "vgi.keywords".to_string(),
                keywords_json(
                    "iso 20022, iso20022, swift mt, payments, pacs.008, pacs.002, pain.001, \
                     camt.053, camt.054, mt103, mt202, mt940, mt942, statement, credit transfer, \
                     reconciliation, cbpr+, iban, bic, uetr, fintech, treasury, aml",
                ),
            ),
            (
                "vgi.doc_llm".to_string(),
                "Parse SWIFT MT (MT103, MT202/COV, MT940, MT942) and ISO 20022 MX (pacs.008, \
                 pacs.002, pain.001, camt.053, camt.054) payment messages into typed rows. Table \
                 functions read a glob of files (one row per transaction / statement / status); \
                 per-message table functions explode statement entries and lines; scalars sniff the \
                 MT type, read MT fields, return the exact settled amount, and validate a message \
                 (structural + IBAN/BIC/currency/charge-bearer/UETR rules). Use for reconciliation, \
                 MT103<->pacs.008 migration QA, and payment feature extraction — all local, no egress."
                    .to_string(),
            ),
            (
                "vgi.doc_md".to_string(),
                "# iso20022 — SWIFT MT & ISO 20022 MX Payment Parsing in SQL\n\n\
                 **Turn SWIFT MT and ISO 20022 MX payment messages into typed, queryable rows \
                 directly in DuckDB.** The worker ATTACHes over Apache Arrow and exposes table \
                 functions that scan a glob of payment files — `mt103_read`, `mt202_read`, \
                 `mt940_read`, `mt942_read`, `pacs008_read`, `pacs002_read`, `pain001_read`, \
                 `camt053_read`, `camt054_read` — plus per-message functions that explode statement \
                 entries and lines (`camt053_entries`, `camt054_entries`, `mt940_lines`, \
                 `mt942_lines`) and scalars for sniffing (`iso20022_mt_type`), field access \
                 (`iso20022_mt103_field`, `iso20022_mt103_amount`), and validation \
                 (`iso20022_validate`).\n\n\
                 It is built for **bank / fintech / payment-processor** data teams doing \
                 reconciliation, ISO 20022 migration dual-running QA (MT103 vs pacs.008 \
                 equivalence), and sanctions/feature extraction. Money is parsed with exact decimals \
                 (no float drift) so amount-equality joins hold; dates use the SWIFT century pivot \
                 and ISO 8601. **Data residency:** the worker makes zero outbound calls and parses \
                 every message locally — debtor/creditor names, IBANs, and amounts never leave the \
                 host.\n\n\
                 The parsers are built on permissive open-source components and the open, freely \
                 published ISO 20022 message standard (iso20022.org); see the \
                 [source repository](https://github.com/Query-farm/vgi-iso20022) for the full \
                 catalog and examples. Part of the [Query.Farm](https://query.farm) VGI ecosystem of \
                 DuckDB workers."
                    .to_string(),
            ),
            (
                "vgi.agent_test_tasks".to_string(),
                agent_test_tasks_json(&[
                    (
                        "worker_version",
                        "What version of the iso20022 worker is running? Return one row with one \
                         column named version.",
                        "SELECT iso20022.main.iso20022_version() AS version",
                    ),
                    (
                        "sniff_mt_type",
                        "Given the inline SWIFT message text \
                         '{1:F01X}{2:I103X}{4:\n:20:R\n:32A:260101EUR1,00\n-}', what MT type is it? \
                         Return a single column named mt.",
                        "SELECT iso20022.main.iso20022_mt_type('{1:F01X}{2:I103X}{4:\n:20:R\n:32A:260101EUR1,00\n-}') AS mt",
                    ),
                    (
                        "validate_ok",
                        "Is the inline message \
                         '{1:F01X}{2:I103X}{4:\n:20:R\n:23B:CRED\n:32A:260101EUR1,00\n:50K:/DE89370400440532013000\nACME\n:59:/FR1420041010050500013M02606\nWIDGETS\n:71A:SHA\n-}' \
                         structurally valid? Return one column named ok.",
                        "SELECT (iso20022.main.iso20022_validate('{1:F01X}{2:I103X}{4:\n:20:R\n:23B:CRED\n:32A:260101EUR1,00\n:50K:/DE89370400440532013000\nACME\n:59:/FR1420041010050500013M02606\nWIDGETS\n:71A:SHA\n-}')).ok AS ok",
                    ),
                ]),
            ),
            ("vgi.author".to_string(), "Query.Farm".to_string()),
            (
                "vgi.copyright".to_string(),
                "Copyright 2026 Query Farm LLC - https://query.farm".to_string(),
            ),
            ("vgi.license".to_string(), "MIT".to_string()),
            (
                "vgi.support_contact".to_string(),
                "https://github.com/Query-farm/vgi-iso20022/issues".to_string(),
            ),
            (
                "vgi.support_policy_url".to_string(),
                "https://github.com/Query-farm/vgi-iso20022/blob/main/README.md".to_string(),
            ),
        ],
        source_url: Some("https://github.com/Query-farm/vgi-iso20022".to_string()),
        schemas: vec![CatSchema {
            name: "main".to_string(),
            comment: Some(
                "SWIFT MT and ISO 20022 MX payment-message parsing functions.".to_string(),
            ),
            tags: vec![
                ("vgi.title".to_string(), "iso20022 — parsing functions".to_string()),
                (
                    "vgi.keywords".to_string(),
                    keywords_json(
                        "iso 20022, swift mt, pacs, pain, camt, mt103, mt940, statement, \
                         credit transfer, reconciliation, validate, mt_type",
                    ),
                ),
                ("domain".to_string(), "payments-and-banking".to_string()),
                ("category".to_string(), "message-parsing".to_string()),
                ("topic".to_string(), "iso20022-swift-mt".to_string()),
                (
                    "vgi.doc_llm".to_string(),
                    "Functions to parse SWIFT MT and ISO 20022 MX payment messages: file-glob \
                     readers (mt103_read … camt054_read), per-message entry/line exploders, and \
                     scalars for type sniffing, field access, exact amount, and validation."
                        .to_string(),
                ),
                (
                    "vgi.doc_md".to_string(),
                    "The single schema for the `iso20022` worker. It holds the file-glob `*_read` \
                     table functions, the per-message `*_entries` / `*_lines` exploders, and the \
                     `iso20022_*` scalar functions (mt_type, mt103_field, mt103_amount, validate, \
                     version)."
                        .to_string(),
                ),
                // Offline-runnable inline examples (the file-glob `*_read`
                // functions scan external files, so they are documented in each
                // function's doc_md rather than executed here — VGI902).
                (
                    "vgi.example_queries".to_string(),
                    "SELECT iso20022.main.iso20022_version();\n\
                     SELECT iso20022.main.iso20022_mt_type('{1:F01X}{2:I103X}{4:\n:20:R\n-}');\n\
                     SELECT iso20022.main.iso20022_mt103_amount('{1:F01X}{2:I103X}{4:\n:20:R\n:32A:260101EUR1234,56\n-}');\n\
                     SELECT (iso20022.main.iso20022_validate('<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08\"><FIToFICstmrCdtTrf><GrpHdr><MsgId>M</MsgId></GrpHdr></FIToFICstmrCdtTrf></Document>')).ok;"
                        .to_string(),
                ),
            ],
            views: Vec::new(),
            macros: Vec::new(),
            tables: Vec::new(),
        }],
        ..Default::default()
    }
}

fn main() {
    // Logs MUST go to stderr — stdout is the Arrow-IPC channel.
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("VGI_LOG", "info"))
        .format_timestamp_millis()
        .try_init();

    if std::env::var_os("VGI_WORKER_CATALOG_NAME").is_none() {
        std::env::set_var("VGI_WORKER_CATALOG_NAME", "iso20022");
    }
    let catalog_name =
        std::env::var("VGI_WORKER_CATALOG_NAME").unwrap_or_else(|_| "iso20022".to_string());

    let mut worker = Worker::new();
    scalar::register(&mut worker);
    table::register(&mut worker);
    table_in_out::register(&mut worker);
    worker.set_catalog(catalog_metadata(&catalog_name));
    worker.run();
}
