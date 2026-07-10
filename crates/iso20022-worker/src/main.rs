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

use iso20022_worker::meta::{agent_test_tasks_json, keywords_json, object_tags, AgentTask};
use iso20022_worker::{scalar, table, table_in_out};
use vgi::catalog::{CatSchema, CatView, CatalogModel};
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
                 directly in DuckDB.** ATTACH the worker over Apache Arrow and parse FIN/MT files \
                 and MX XML documents locally — one typed row per message, transaction, or statement \
                 entry — so payments become plain relational data instead of something you feed to a \
                 bespoke parser.\n\n\
                 It is built for **bank / fintech / payment-processor** data teams doing \
                 reconciliation, ISO 20022 migration dual-running QA (MT vs MX equivalence), and \
                 sanctions/feature extraction. Money is parsed with exact decimals (no float drift) \
                 so amount-equality joins hold; dates use the SWIFT century pivot and ISO 8601. \
                 **Data residency:** the worker makes zero outbound calls and parses every message \
                 locally — debtor/creditor names, IBANs, and amounts never leave the host.\n\n\
                 Reach for it whenever you have MT statements or transfers, or camt/pacs/pain XML, \
                 on disk (or inline) and want them as rows: list the schema to discover the file \
                 readers, statement exploders, and inspection scalars it provides. The parsers are \
                 built on permissive open-source components and the open, freely published ISO 20022 \
                 message standard (iso20022.org); see the \
                 [source repository](https://github.com/Query-farm/vgi-iso20022) for the full \
                 catalog and examples. Part of the [Query.Farm](https://query.farm) VGI ecosystem of \
                 DuckDB workers."
                    .to_string(),
            ),
            (
                "vgi.agent_test_tasks".to_string(),
                agent_test_tasks_json(&agent_tasks()),
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
                // Ordered navigation registry (VGI413). Each object carries a
                // matching `vgi.category` tag naming exactly one of these.
                (
                    "vgi.categories".to_string(),
                    "[{\"name\":\"Message readers\",\
                       \"description\":\"File-glob table functions that scan payment message files \
                       on the worker host and return one typed row per message, transaction, or \
                       statement.\"},\
                      {\"name\":\"Statement exploders\",\
                       \"description\":\"Per-message table functions that explode a statement or \
                       notification into its individual entries or lines, passing every input \
                       column through so children correlate back to the parent.\"},\
                      {\"name\":\"Message inspection\",\
                       \"description\":\"Scalar functions that sniff the type of, extract fields \
                       and the settled amount from, validate, and report the version of a single \
                       inline message.\"},\
                      {\"name\":\"Discovery\",\
                       \"description\":\"Browsable registry views that let an agent see which \
                       message types the worker supports and which reader / exploder function \
                       handles each, without guessing.\"}]"
                        .to_string(),
                ),
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
                    "## The `iso20022` worker schema\n\n\
                     This is the single schema for the worker. Everything it exposes turns SWIFT \
                     MT and ISO 20022 MX payment messages into typed, relational rows you can query \
                     directly in SQL, with **no network access and no egress** — messages are \
                     parsed on the host.\n\n\
                     The objects fall into three groups:\n\n\
                     - **Message readers** scan a glob of message files on disk and return one \
                       typed row per message, transaction, or statement.\n\
                     - **Statement exploders** take an already-selected statement or notification \
                       and unnest it into its individual entries or lines, carrying every input \
                       column through so children correlate back to the parent.\n\
                     - **Message inspection** scalars sniff the message type, pull individual \
                       fields and the exact settled amount out of a single inline message, \
                       validate it against structural and CBPR+ rules, and report the worker \
                       version.\n\n\
                     Money is parsed with exact decimals so amount-equality joins hold, and dates \
                     use the SWIFT century pivot plus ISO 8601. List the schema to discover the \
                     concrete functions and their documented examples."
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
            views: vec![supported_messages_view()],
            macros: Vec::new(),
            tables: Vec::new(),
        }],
        ..Default::default()
    }
}

// --- Inline sample messages for the agent-suitability suite ------------------
// Fabricated names / IBANs / BICs (no real PII). The MT messages use the SWIFT
// `\n` line structure; the MX messages are compact single-line XML so an analyst
// can paste them straight into SQL. Each reader accepts inline text, so a task
// exercises the real parse path without a data file present.

const VALID_MT103: &str = "{1:F01X}{2:I103X}{4:\n:20:R\n:23B:CRED\n:32A:260101EUR1,00\n:50K:/DE89370400440532013000\nACME\n:59:/FR1420041010050500013M02606\nWIDGETS\n:71A:SHA\n-}";

const MT103: &str = "{1:F01ACMEDEFFAXXX0000000000}{2:I103DEUTDEFFXXXXN}{3:{121:e3bf1c2a-1111-4aaa-8bbb-1234567890ab}}{4:\n:20:TXN-REF-1\n:23B:CRED\n:32A:260101EUR1234,56\n:50K:/DE89370400440532013000\nACME CORP\n:59:/FR1420041010050500013M02606\nWIDGETS SARL\n:70:INVOICE 998877\n:71A:SHA\n:72:/EToE/E2E-REF-001\n-}";

const MT202: &str = "{1:F01ACMEDEFFAXXX0000000000}{2:I202DEUTDEFFXXXXN}{4:\n:20:FI-REF-9\n:21:REL-REF-9\n:32A:260102USD5000000,00\n:52A:CHASUS33\n:58A:DEUTDEFF\n:50K:/111\nUNDERLYING DEBTOR\n:59:/222\nUNDERLYING CREDITOR\n:70:COVER FOR MT103\n-}";

const MT940: &str = "{1:F01ACMEDEFFAXXX0000000000}{2:O940DEUTDEFFXXXXN}{4:\n:20:STMT-1\n:25:DE89370400440532013000\n:28C:12345/1\n:60F:C260101EUR1000,00\n:61:2601020102C500,00NTRFNONREF//BANK-A\nEXTRA INFO\n:86:GROCERY STORE PAYMENT\n:61:2601030103D250,50NMSCCUST-REF//BANK-B\n:86:?20RENT ?30BANK ?31ACC123\n:62F:C260103EUR1249,50\n:64:C260103EUR1249,50\n-}";

const MT942: &str = "{1:F01ACMEDEFFAXXX0000000000}{2:O942DEUTDEFFXXXXN}{4:\n:20:INTERIM-1\n:25:DE89370400440532013000\n:28C:99/1\n:34F:EURD0,00\n:34F:EURC1000,00\n:13D:2601021430+0100\n:61:2601020102C500,00NTRFNONREF//BANK-A\n:86:INCOMING\n:90D:5EUR2500,00\n:90C:3EUR1500,00\n-}";

const PACS008: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>PACS-MSG-1</MsgId><CreDtTm>2026-01-01T10:00:00Z</CreDtTm><NbOfTxs>1</NbOfTxs><SttlmInf><SttlmMtd>INDA</SttlmMtd></SttlmInf></GrpHdr><CdtTrfTxInf><PmtId><InstrId>INSTR-1</InstrId><EndToEndId>E2E-REF-001</EndToEndId><UETR>e3bf1c2a-1111-4aaa-8bbb-1234567890ab</UETR></PmtId><IntrBkSttlmAmt Ccy="EUR">1234.56</IntrBkSttlmAmt><IntrBkSttlmDt>2026-01-01</IntrBkSttlmDt><ChrgBr>SHAR</ChrgBr><Dbtr><Nm>ACME CORP</Nm></Dbtr><DbtrAcct><Id><IBAN>DE89370400440532013000</IBAN></Id></DbtrAcct><DbtrAgt><FinInstnId><BICFI>DEUTDEFF</BICFI></FinInstnId></DbtrAgt><Cdtr><Nm>WIDGETS SARL</Nm></Cdtr><CdtrAcct><Id><IBAN>FR1420041010050500013M02606</IBAN></Id></CdtrAcct><CdtrAgt><FinInstnId><BICFI>BNPAFRPP</BICFI></FinInstnId></CdtrAgt><Purp><Cd>GDDS</Cd></Purp><RmtInf><Ustrd>INVOICE 998877</Ustrd></RmtInf></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;

const PACS002: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.002.001.10"><FIToFIPmtStsRpt><GrpHdr><MsgId>STS-1</MsgId><CreDtTm>2026-01-01T11:00:00Z</CreDtTm></GrpHdr><OrgnlGrpInfAndSts><OrgnlMsgId>PACS-MSG-1</OrgnlMsgId><OrgnlMsgNmId>pacs.008.001.08</OrgnlMsgNmId></OrgnlGrpInfAndSts><TxInfAndSts><OrgnlEndToEndId>E2E-REF-001</OrgnlEndToEndId><OrgnlUETR>e3bf1c2a-1111-4aaa-8bbb-1234567890ab</OrgnlUETR><TxSts>RJCT</TxSts><StsRsnInf><Rsn><Cd>AM04</Cd></Rsn><AddtlInf>Insufficient funds</AddtlInf></StsRsnInf></TxInfAndSts></FIToFIPmtStsRpt></Document>"#;

const PAIN001: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pain.001.001.09"><CstmrCdtTrfInitn><GrpHdr><MsgId>PAIN-1</MsgId><CreDtTm>2026-01-01T09:00:00Z</CreDtTm><CtrlSum>1234.56</CtrlSum><InitgPty><Nm>ACME CORP</Nm></InitgPty></GrpHdr><PmtInf><PmtInfId>PMT-1</PmtInfId><PmtMtd>TRF</PmtMtd><ReqdExctnDt><Dt>2026-01-02</Dt></ReqdExctnDt><Dbtr><Nm>ACME CORP</Nm></Dbtr><DbtrAcct><Id><IBAN>DE89370400440532013000</IBAN></Id></DbtrAcct><DbtrAgt><FinInstnId><BICFI>DEUTDEFF</BICFI></FinInstnId></DbtrAgt><CdtTrfTxInf><PmtId><EndToEndId>E2E-REF-001</EndToEndId></PmtId><Amt><InstdAmt Ccy="EUR">1234.56</InstdAmt></Amt><Cdtr><Nm>WIDGETS SARL</Nm></Cdtr><CdtrAcct><Id><IBAN>FR1420041010050500013M02606</IBAN></Id></CdtrAcct><RmtInf><Ustrd>INVOICE 998877</Ustrd></RmtInf></CdtTrfTxInf></PmtInf></CstmrCdtTrfInitn></Document>"#;

const CAMT053: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08"><BkToCstmrStmt><GrpHdr><MsgId>CAMT053-1</MsgId><CreDtTm>2026-01-03T06:00:00Z</CreDtTm></GrpHdr><Stmt><Id>STMT-1</Id><ElctrncSeqNb>5</ElctrncSeqNb><Acct><Id><IBAN>DE89370400440532013000</IBAN></Id><Ccy>EUR</Ccy><Ownr><Nm>ACME CORP</Nm></Ownr></Acct><Bal><Tp><CdOrPrtry><Cd>CLBD</Cd></CdOrPrtry></Tp><Amt Ccy="EUR">1500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd></Bal><Ntry><Amt Ccy="EUR">500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd><Sts><Cd>BOOK</Cd></Sts><BookgDt><Dt>2026-01-02</Dt></BookgDt><ValDt><Dt>2026-01-02</Dt></ValDt><NtryDtls><TxDtls><Refs><EndToEndId>E2E-REF-001</EndToEndId></Refs></TxDtls></NtryDtls></Ntry></Stmt></BkToCstmrStmt></Document>"#;

const CAMT054: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.054.001.08"><BkToCstmrDbtCdtNtfctn><GrpHdr><MsgId>CAMT054-1</MsgId><CreDtTm>2026-01-03T06:30:00Z</CreDtTm></GrpHdr><Ntfctn><Id>NTF-1</Id><Acct><Id><IBAN>DE89370400440532013000</IBAN></Id><Ccy>EUR</Ccy></Acct><Ntry><Amt Ccy="EUR">250.50</Amt><CdtDbtInd>DBIT</CdtDbtInd><Sts><Cd>BOOK</Cd></Sts><NtryDtls><TxDtls><Refs><EndToEndId>E2E-DEBIT-9</EndToEndId></Refs></TxDtls></NtryDtls></Ntry></Ntfctn></BkToCstmrDbtCdtNtfctn></Document>"#;

/// The catalog-level `vgi.agent_test_tasks` suite (VGI520 coverage / VGI920
/// agent-simulation). Every registered object — the five scalars, the nine file
/// readers, the four statement exploders, and the `supported_messages` view — is
/// exercised by at least one task. Deterministic answers (counts, booleans,
/// single fields) keep result-compare grading stable; `reference_sql` is the
/// grader-only canonical solution and never shown to the analyst.
fn agent_tasks() -> Vec<AgentTask> {
    vec![
        AgentTask::exact(
            "worker_version",
            "What version string does the iso20022 worker report? Return one row with one column \
             named version.",
            "SELECT iso20022.main.iso20022_version() AS version",
        ),
        AgentTask::exact(
            "sniff_mt_type",
            format!(
                "A payment arrived as this inline SWIFT FIN message:\n{VALID_MT103}\nWhich SWIFT MT \
                 message type is it? Return one column named mt."
            ),
            format!("SELECT iso20022.main.iso20022_mt_type('{VALID_MT103}') AS mt"),
        ),
        AgentTask::exact(
            "mt103_amount_threshold",
            format!(
                "Does this inline MT103 settle for more than 1000.00 in its :32A: amount field? \
                 Message:\n{MT103}\nReturn one boolean column named over_1000."
            ),
            format!("SELECT iso20022.main.iso20022_mt103_amount('{MT103}') > 1000 AS over_1000"),
        ),
        AgentTask::exact(
            "mt103_charge_bearer",
            format!(
                "Read the charge-bearer code carried in field :71A: of this inline MT103. \
                 Message:\n{MT103}\nReturn one column named charges."
            ),
            format!("SELECT trim(iso20022.main.iso20022_mt103_field('{MT103}', '71A')) AS charges"),
        ),
        AgentTask::exact(
            "validate_ok",
            format!(
                "Is this inline SWIFT message structurally valid? Message:\n{VALID_MT103}\nReturn \
                 one boolean column named ok."
            ),
            format!("SELECT (iso20022.main.iso20022_validate('{VALID_MT103}')).ok AS ok"),
        ),
        AgentTask::exact(
            "mt103_read_currency",
            format!(
                "Parse this inline MT103 message and report the currency it settles in (the \
                 worker's message readers accept inline text). Message:\n{MT103}\nReturn one \
                 column named ccy."
            ),
            format!("SELECT ccy FROM iso20022.main.mt103_read('{MT103}')"),
        ),
        AgentTask::exact(
            "mt202_amount_check",
            format!(
                "Parse this inline MT202 cover message and decide whether its settled amount \
                 exceeds 1000000 (the readers accept inline text). Message:\n{MT202}\nReturn one \
                 boolean column named big."
            ),
            format!("SELECT amount > 1000000 AS big FROM iso20022.main.mt202_read('{MT202}')"),
        ),
        AgentTask::exact(
            "mt940_line_count",
            format!(
                "How many statement lines does this inline MT940 end-of-day statement contain \
                 (each :61: line is one row)? The readers accept inline text. Message:\n{MT940}\n\
                 Return one column named n."
            ),
            format!(
                "SELECT count(*) AS n FROM iso20022.main.mt940_lines((SELECT raw FROM \
                 iso20022.main.mt940_read('{MT940}')))"
            ),
        ),
        AgentTask::exact(
            "mt942_line_count",
            format!(
                "How many transaction lines does this inline MT942 interim report contain? The \
                 readers accept inline text. Message:\n{MT942}\nReturn one column named n."
            ),
            format!(
                "SELECT count(*) AS n FROM iso20022.main.mt942_lines((SELECT raw FROM \
                 iso20022.main.mt942_read('{MT942}')))"
            ),
        ),
        AgentTask::exact(
            "pacs008_uetr",
            format!(
                "Parse this inline pacs.008 credit transfer and return its UETR (the readers accept \
                 inline XML text). Message:\n{PACS008}\nReturn one column named uetr."
            ),
            format!("SELECT uetr FROM iso20022.main.pacs008_read('{PACS008}')"),
        ),
        AgentTask::exact(
            "pacs002_orig_uetr",
            format!(
                "This inline pacs.002 status report references an original payment. Return that \
                 original UETR. Message:\n{PACS002}\nReturn one column named orig_uetr."
            ),
            format!("SELECT orig_uetr FROM iso20022.main.pacs002_read('{PACS002}')"),
        ),
        AgentTask::exact(
            "pain001_currency",
            format!(
                "What currency is this inline pain.001 credit-transfer initiation in? Message:\n\
                 {PAIN001}\nReturn one column named ccy."
            ),
            format!("SELECT ccy FROM iso20022.main.pain001_read('{PAIN001}')"),
        ),
        AgentTask::exact(
            "camt053_booked_entries",
            format!(
                "How many booked (status BOOK) entries does this inline camt.053 statement \
                 contain? Explode its entries. Message:\n{CAMT053}\nReturn one column named n."
            ),
            format!(
                "SELECT count(*) AS n FROM iso20022.main.camt053_entries((SELECT raw FROM \
                 iso20022.main.camt053_read('{CAMT053}'))) WHERE status = 'BOOK'"
            ),
        ),
        AgentTask::exact(
            "camt054_debit_entries",
            format!(
                "How many debit (DBIT) entries does this inline camt.054 notification contain? \
                 Explode its entries. Message:\n{CAMT054}\nReturn one column named n."
            ),
            format!(
                "SELECT count(*) AS n FROM iso20022.main.camt054_entries((SELECT raw FROM \
                 iso20022.main.camt054_read('{CAMT054}'))) WHERE credit_debit = 'DBIT'"
            ),
        ),
        AgentTask::exact(
            "supported_messages_with_exploder",
            "Which message types does the iso20022 worker expose a dedicated statement or \
             notification exploder function for? List each message_type, sorted ascending. Return \
             one column named message_type.",
            "SELECT message_type FROM iso20022.main.supported_messages WHERE exploder_function IS \
             NOT NULL ORDER BY message_type",
        ),
    ]
}

/// A VALUES-backed browsable registry of the message types the worker parses.
/// A real view (not a table-function wrapper) so it satisfies VGI146, scans with
/// no network / credential (clearing VGI911 for free), and gives an agent a
/// concrete relation to inspect before it calls any reader.
fn supported_messages_view() -> CatView {
    let definition = "SELECT * FROM (VALUES \
        ('MT103', 'SWIFT MT', 'mt103_read', CAST(NULL AS VARCHAR), 'Single customer credit transfer (FIN MT103).'), \
        ('MT202', 'SWIFT MT', 'mt202_read', CAST(NULL AS VARCHAR), 'General financial-institution transfer and cover payment (MT202/COV).'), \
        ('MT940', 'SWIFT MT', 'mt940_read', 'mt940_lines', 'Customer statement, end of day.'), \
        ('MT942', 'SWIFT MT', 'mt942_read', 'mt942_lines', 'Interim transaction report, intra-day.'), \
        ('pacs.008', 'ISO 20022 MX', 'pacs008_read', CAST(NULL AS VARCHAR), 'FI-to-FI customer credit transfer.'), \
        ('pacs.002', 'ISO 20022 MX', 'pacs002_read', CAST(NULL AS VARCHAR), 'Payment status report.'), \
        ('pain.001', 'ISO 20022 MX', 'pain001_read', CAST(NULL AS VARCHAR), 'Customer credit-transfer initiation.'), \
        ('camt.053', 'ISO 20022 MX', 'camt053_read', 'camt053_entries', 'Bank-to-customer statement.'), \
        ('camt.054', 'ISO 20022 MX', 'camt054_read', 'camt054_entries', 'Bank-to-customer debit/credit notification.') \
        ) AS t(message_type, family, reader_function, exploder_function, description)"
        .to_string();

    let mut tags = object_tags(
        "Supported Message Registry",
        "A browsable registry of every SWIFT MT and ISO 20022 MX message type this worker parses. \
         One row per message type carries its family (SWIFT MT / ISO 20022 MX), the reader function \
         that turns the message (inline text, a file path, or a glob) into rows, the optional \
         exploder function that unnests its entries or lines, and a one-line description. Query it \
         to discover what the worker supports before guessing reader arguments.",
        "## supported_messages\n\nA browsable registry of the message types this worker parses — \
         one row per type with its `family`, `reader_function`, optional `exploder_function`, and a \
         short `description`. Filter it (for example on `exploder_function IS NOT NULL`) to find \
         which messages have an entry/line exploder. Local-only, no network.",
        "supported messages, registry, catalog, discovery, message types, readers, exploders, mt, \
         mx, iso 20022, swift",
    );
    tags.push(("vgi.category".into(), "Discovery".into()));
    tags.push(("domain".into(), "payments-and-banking".into()));
    tags.push(("topic".into(), "iso20022-swift-mt".into()));
    tags.push((
        "vgi.example_queries".into(),
        r#"[{"description":"List the message types that have a dedicated entry/line exploder function, with their reader.","sql":"SELECT message_type, reader_function, exploder_function FROM iso20022.main.supported_messages WHERE exploder_function IS NOT NULL ORDER BY message_type"}]"#.into(),
    ));

    CatView {
        name: "supported_messages".into(),
        definition,
        comment: Some(
            "Registry of the SWIFT MT and ISO 20022 MX message types this worker parses, with the \
             reader and exploder function for each."
                .into(),
        ),
        tags,
        column_comments: vec![
            (
                "message_type".into(),
                "The message identifier, e.g. 'MT103' or 'pacs.008'.".into(),
            ),
            (
                "family".into(),
                "The standard family: 'SWIFT MT' or 'ISO 20022 MX'.".into(),
            ),
            (
                "reader_function".into(),
                "The table function that reads this message (inline text, a file path, or a glob) \
                 into typed rows."
                    .into(),
            ),
            (
                "exploder_function".into(),
                "The per-message table function that unnests this message's entries or lines, or \
                 NULL when it has none."
                    .into(),
            ),
            (
                "description".into(),
                "A one-line description of the message type.".into(),
            ),
        ],
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
