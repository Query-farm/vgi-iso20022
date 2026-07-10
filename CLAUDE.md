# CLAUDE.md — vgi-iso20022

Guidance for working in this repo. It is a Rust **VGI worker** that parses SWIFT
MT and ISO 20022 MX payment messages into typed DuckDB rows over Apache Arrow.

## Layout

Cargo workspace, two crates:

- `crates/iso20022-core` — **pure parse engine**, no Arrow / VGI deps, no network.
  - `sniff.rs` MT-vs-MX + type detection; `money.rs` exact `Decimal` (comma→dot,
    `DECIMAL(38,9)` bridge); `dates.rs` `YYMMDD` century pivot + ISO 8601;
    `charset.rs` SWIFT X-set sanitize; `cursor.rs` serde `FileGlobCursor`.
  - `mt/` block-4 tag parser (`block.rs`), field/party/amount helpers
    (`field.rs`), `uetr.rs`, and `mt103/mt202/mt940/mt942.rs`.
  - `mx/` a tiny namespace-agnostic DOM over `quick-xml` (`dom.rs`), shared
    helpers (`common.rs`), and `pacs008/pacs002/pain001/camt053/camt054.rs`
    (camt entries shared in `camt.rs`).
  - `validate.rs` structural + CBPR+ field rules (IBAN mod-97, BIC, ccy, charge
    bearer, UETR).
  - Every public entry point is **total** — malformed input yields empty/partial
    results, never a panic (`tests/fuzz.rs` proptest, 2000 cases).
- `crates/iso20022-worker` — **thin Arrow adapters** + the binary.
  - `scalar/` `iso20022_version`, `iso20022_mt_type`, `iso20022_mt103_field`,
    `iso20022_mt103_amount`, `iso20022_validate`.
  - `table/` the nine file-glob `*_read` functions. One generic `ReadTable`
    (in `common.rs`) implements `TableFunction` once; each message module
    supplies `schema()` + `build(path, content) -> RecordBatch`. `scan.rs` holds
    the `GlobProducer` + externalized-cursor glue.
  - `table_in_out/` the per-message exploders (`*_entries`, `*_lines`). One
    generic `PerMessageTable` implements `TableInOutFunction`; `entries.rs` /
    `lines.rs` supply the child schema + `build(&[String]) -> (counts, columns)`.
  - `cols.rs` typed Arrow column builders (str/dec/date/ts/bool/int/list/map);
    `arrow_io.rs` cell readers + the path|text|bytes message resolver; `meta.rs`
    the `vgi.*` metadata-tag helpers; `main.rs` the catalog model + registration.

## Hard-won gotchas (read before editing the worker)

- **Schema is `main`.** Registered VGI functions always live in schema `main`
  (the SDK constant `MAIN_SCHEMA`); the `CatSchema` name is metadata only. Qualify
  examples as `iso20022.main.<fn>`, and put schema-level `vgi.*` tags on the
  `main` schema in `main.rs`.
- **Per-message exploders take a RELATION, not a lateral scalar.** DuckDB in-out
  functions don't support `LATERAL tf(s.raw)`. Call them as
  `tf((SELECT cols…, raw FROM …))`; arg 0 must be `ArgSpec::column(.., "table", ..)`.
  Every input column is passed through (via `arrow_select::take`), repeated once
  per child row, so results correlate back to the parent.
- **MAP field names.** DuckDB expects a MAP's nested struct fields named
  `key`/`value` (singular); arrow's `MapBuilder` defaults to `keys`/`values`.
  `cols.rs` sets explicit `MapFieldNames` so the declared schema matches the built
  array after the DuckDB bind round-trip.
- **List/Map types are derived from an empty builder** in `cols.rs` so the nested
  field names always match what the builder emits (avoids `try_new` type errors).
- **Scalar output type is fixed at bind.** `iso20022_validate` returns a
  fixed-schema `STRUCT(ok BOOLEAN, errors VARCHAR[])` — fine. A hypothetical
  "decode any message" scalar would have to return canonical JSON (VARCHAR), not a
  variable STRUCT.
- **`chrono` has no `clock` feature** here (no `Utc::now()`); every date is parsed
  from message bytes.

## Gates (all must be green)

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace
# vgi-lint metadata gate (must be clean at --fail-on info). Unpinned — always the
# latest published vgi-lint-check, like CI's Query-farm/vgi-lint-check@v1 action.
# Add --execute --ai --ai-concurrency 1 to also run the VGI9xx execution rules,
# the VGI180 doc-quality judge, and the VGI920 agent-simulation (needs the local
# `claude` CLI; slow):
uvx --prerelease=allow --from vgi-lint-check vgi-lint lint \
    "$PWD/target/release/iso20022-worker" --fail-on info --no-check-links
# SQL E2E across transports (needs the community vgi extension + haybarn-unittest):
make test-sql
```

CI (`.github/workflows/ci.yml`) runs fmt/clippy/build/doc/tests, the SQL E2E
matrix (subprocess/http/unix × ubuntu/macos via `haybarn-unittest` + the signed
community `vgi` extension), and `Query-farm/vgi-lint-check@v1` at `fail-on=info`.
Releases go through `Query-farm/vgi-actions/.github/workflows/rust-release.yml@v1`.

## Conventions

- LICENSE is **MIT** (fleet standard). No network, no secrets, no egress.
- Permissive deps only — never the commercial SWIFT MyStandards SDK or LGPL
  `prowide-core`. ISO 20022 XSDs are an open standard.
- Money is `rust_decimal::Decimal` end to end; never `f64`.
- Golden fixtures (`data/`) use fabricated names/IBANs/BICs — no real PII.
