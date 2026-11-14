# ISO 20022 notebooks

[01 · Reconcile statement entries, not repeated transaction details](01_reconcile_entries_not_details.ipynb) shows why a camt.053 entry's amount must be counted once even when its transaction details expand to several rows. Exact decimal reconciliation then exposes the effect of filtering small entries.

## Run

Install [uv](https://docs.astral.sh/uv/). Python 3.13–3.14 is required; uv can install it. Initial package, extension and worker-release downloads need internet. The worker itself parses local data without network access. No local worker build, account or credentials are needed.

```sh
cd notebooks
uv sync --locked
uv run --locked python execute.py --timeout 300
```

The visible literal connection is `github-auto://Query-farm/vgi-iso20022@v0.4.0`, tested through Haybarn 2.0.0rc11. The release tag resolves to `842cfbf351731f42a46488e56d0569cff4e73a3c`; the extension selects a platform artifact, checks its SHA-256 sidecar and caches it. The release provides macOS arm64 and Linux x86-64/arm64. Upgrade the pin deliberately and rerun both populations.

Optional editor kernel:

```sh
uv run --locked python -m ipykernel install --user --name vgi-iso20022-notebooks --display-name "Python (vgi-iso20022 notebooks)"
```

Use this directory as the working directory and Restart Kernel → Run All. The runner creates a temporary kernel and preserves the last successful saved notebook if any cell fails.

## Experiment

```sh
uv run --locked python execute.py --parameters '{"MIN_ENTRY_AMOUNT":"100.00"}' --output-dir artifacts/minimum-100
```

Restore `"0.00"` for the default. Override runs require a separate output destination. The runner copies fixtures, documentation, lock and branding beside the executed copy. Environments, validation images, logs and generated experiments are ignored.

## Inputs and analytical contract

`fixtures/eur.xml` and `fixtures/usd.xml` are explicitly synthetic teaching statements, with fabricated account identifiers and no personal data. `manifest.json` records their SHA-256 hashes. Both state 2026-01-02 00:00:00 through 23:59:59 UTC inclusive, with a message creation time of 2026-01-03 06:00:00 UTC. Booking and value fields are dates. They include multiple TxDtls per entry, an entry without details, a pending entry, a debit reversal and an opening debit balance. They are parser inputs, not claims of full schema or payment-network validation.

The [release parser](https://github.com/Query-farm/vgi-iso20022/blob/v0.4.0/crates/iso20022-core/src/mx/camt.rs) repeats `Ntry/Amt` for each TxDtls. SQL verifies entry-level consistency, recovers one row per `(stmt_id, entry_idx)`, applies CRDT/DBIT sign once, excludes PDNG, and keeps account/balance/entry currencies equal. It does not infer individual transaction amounts from a batch total. Reversal does not negate the credit/debit sign a second time. Every money calculation and exact-zero assertion stays in SQL DECIMAL(38,9); visible money tables cast to strings for exact display. Only chart drawing converts to floating point.

The full identity is opening balance plus signed booked entries equals reported closing balance. The filtered identity intentionally retains the complete header balances, so its residual equals the signed movement omitted by the threshold. This illustrates a population mismatch, not a bank error or a recommended accounting threshold. EUR and USD are never summed or converted. Missing/unknown required fields fail checks rather than becoming zero; an empty selected population is the distinct case of a known zero movement sum.

## Pinned-release scope

Version 0.4.0 predates the source fix that narrows `raw` to each statement when a file contains several statements. These fixtures contain exactly one Stmt per file and setup verifies that constraint. Do not reuse this recipe for multi-statement files on that release without independently verifying the child-row mapping. Statement IDs are unique within this demonstration; a production key needs source/account/statement identity and duplicate-file handling.

The notebook establishes parser behavior and arithmetic for these fixtures. It does not establish completeness beyond these files, validate a bank's ledger, perform full ISO 20022/CBPR+ network validation, infer settlement finality or transmit payments. There is no replay/live switch because this is a local computational worker.
