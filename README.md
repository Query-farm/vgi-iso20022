# vgi-iso20022

A [VGI](https://query.farm) worker that parses **SWIFT MT** (MT103, MT202/COV,
MT940, MT942) and **ISO 20022 MX** (pacs.008, pacs.002, pain.001, camt.053,
camt.054) payment messages into typed, queryable rows. DuckDB ATTACHes the worker
over Apache Arrow IPC; you `SELECT` against table functions that take a glob of
message files (or an inline message) and emit one row per message plus child rows
per statement-line / transaction.

**Pure compute, fully local: no network, no API keys, no message data ever leaves
the host** — a selling point for regulated payment data (debtor/creditor names,
IBANs, amounts are PII).

- **Language / SDK:** Rust, [`vgi`](https://crates.io/crates/vgi) 0.9.5 (arrow 59).
- **Catalog:** `iso20022`, **schema:** `main` (registered VGI functions always
  live in `main`; the catalog name is what you choose in `ATTACH`).
- **License:** MIT.

## Install & attach

```sql
INSTALL vgi FROM community;
LOAD vgi;

-- the worker is a local binary; no secret needed (pure parser, no auth)
ATTACH 'iso20022' AS pay (TYPE vgi, COMMAND 'iso20022-worker');
```

## SQL surface

```sql
-- 1) Parse a directory of camt.053 statements -> one row per statement
SELECT msg_id, account_iban, opening_balance, closing_balance, ccy, entry_count
FROM pay.main.camt053_read('/data/statements/*.xml');

-- 2) Explode camt.053 entries (child rows) for reconciliation. The per-message
--    exploders take the relation to explode as a subquery; every input column is
--    passed through (repeated per entry) so the account correlates back.
SELECT account_iban, amount, ccy, credit_debit, value_date, end_to_end_id
FROM pay.main.camt053_entries(
       (SELECT account_iban, raw FROM pay.main.camt053_read('/data/statements/2026-06-*.xml')))
WHERE credit_debit = 'CRDT';

-- 3) Parse SWIFT MT103 blobs straight from a column (scalar)
SELECT iso20022_mt_type(content)                    AS mt,
       iso20022_mt103_field(content, '50K')         AS ordering_customer,
       iso20022_mt103_field(content, '59')          AS beneficiary,
       iso20022_mt103_amount(content)               AS settled_amount
FROM read_text('/inbox/mt103/*.txt');

-- 4) MT103 <-> pacs.008 dual-running equivalence check (migration QA)
SELECT m.end_to_end_id, m.amount AS mt_amount, p.amount AS mx_amount
FROM pay.main.mt103_read('/dual/mt/*.txt') m
JOIN pay.main.pacs008_read('/dual/mx/*.xml') p USING (end_to_end_id)
WHERE m.amount <> p.amount;

-- 5) Structural + CBPR+ field validation, surfaced inline
SELECT path,
       (iso20022_validate(content)).ok,
       (iso20022_validate(content)).errors
FROM read_text('/inbox/**/*.xml');
```

> **Note on the message argument.** Scalars and the `*_entries` / `*_lines`
> exploders accept a message as **inline text**, a **path** to a file on the
> worker host, or **BLOB** bytes. `read_text(...)` is a DuckDB *table* function
> that yields a `content` column — pass `content` (not `raw`) to the scalars when
> reading files that way.

## Function catalog

| Area | Function | Kind |
|---|---|---|
| Version | `iso20022_version() -> VARCHAR` | scalar |
| MT type sniff | `iso20022_mt_type(message) -> VARCHAR` | scalar |
| MT field by tag | `iso20022_mt103_field(message, tag) -> VARCHAR` | scalar |
| MT settled amount | `iso20022_mt103_amount(message) -> DECIMAL(38,9)` | scalar |
| Validate | `iso20022_validate(message) -> STRUCT(ok BOOLEAN, errors VARCHAR[])` | scalar |
| MT103 | `mt103_read(glob) -> TABLE` | table fn |
| MT202 / COV | `mt202_read(glob) -> TABLE` | table fn |
| MT940 statement | `mt940_read(glob) -> TABLE`; `mt940_lines(input) -> TABLE` | table fn + table-in-out |
| MT942 interim | `mt942_read(glob) -> TABLE`; `mt942_lines(input) -> TABLE` | table fn + table-in-out |
| pacs.008 | `pacs008_read(glob) -> TABLE` (one row / `CdtTrfTxInf`) | table fn |
| pacs.002 | `pacs002_read(glob) -> TABLE` (one row / `TxInfAndSts`) | table fn |
| pain.001 | `pain001_read(glob) -> TABLE` (one row / `CdtTrfTxInf`) | table fn |
| camt.053 | `camt053_read(glob) -> TABLE`; `camt053_entries(input) -> TABLE` | table fn + table-in-out |
| camt.054 | `camt054_read(glob) -> TABLE`; `camt054_entries(input) -> TABLE` | table fn + table-in-out |

The `*_read` table functions stream via VGI's **externalized cursor**
(`FileGlobCursor { paths, idx, inner_row }`), serializable so DuckDB can suspend /
resume / parallelize a paginated scan. Every glob table function also carries a
passthrough `raw` (the original message text) and `path` so the child `*_entries`
/ `*_lines` functions and provenance joins work. Money is parsed with
`rust_decimal::Decimal` (never `f64`), so the §4 amount-equality joins are exact.

### Per-message exploders

DuckDB in-out (table-in-out) functions take an **input relation** (a subquery),
not a correlated `LATERAL` scalar. Call `*_entries` / `*_lines` with the relation
to explode; every input column you select is passed through, repeated once per
child row:

```sql
SELECT account, value_date, credit_debit, amount, narrative
FROM pay.main.mt940_lines(
       (SELECT account, raw FROM pay.main.mt940_read('/data/*.txt')));
```

## Validation scope (honest framing)

`iso20022_validate` runs **structural + selected CBPR+ SR2025 field rules**:
balanced MT blocks / well-formed MX, mandatory tags & elements, BIC format, IBAN
mod-97 checksum, ISO 4217 currency + minor-unit consistency, charge-bearer code
sets, and UETR UUID shape. Each error is a stable `CODE: human text` string. This
is **not** full CBPR+/HVPS+ network validation — a green `ok` means "structurally
sound and passes the common field rules", not "the SWIFT network will accept it".

## Build & test

```bash
cargo build --release --bin iso20022-worker   # the worker binary
cargo test --workspace --all-features          # unit + Arrow-boundary + proptest
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

SQL end-to-end (DuckDB `vgi` extension + the committed golden fixtures under
`data/`), across every transport (subprocess / unix / http):

```bash
make test-sql        # or: HAYBARN_UNITTEST=… WORKER_BIN=… TRANSPORT=subprocess ci/run-integration.sh
```

## Licensing & data residency

All dependencies are permissive (MIT / Apache-2.0): `quick-xml`, `rust_decimal`,
`chrono`, `serde`, `arrow`. The ISO 20022 message definitions are a freely
published open standard (iso20022.org). The worker does **not** use the commercial
SWIFT MyStandards SDK, nor LGPL `prowide-core`. It makes **zero outbound calls**
and registers **no secret provider** — it reads only the local files named in a
glob argument or the message passed in SQL. Nothing about a payment leaves the
host.

Part of the [Query.Farm](https://query.farm) VGI ecosystem. Copyright 2026
Query Farm LLC.
