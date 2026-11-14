# ISO 20022 Payments examples

[Worker repository](https://github.com/Query-farm/vgi-iso20022) · [Shared notebook tools and standard](https://github.com/Query-farm/vgi-example-notebooks)

| Notebook | Investigation | Run |
|---|---|---|
| [Reconcile statement entries, not repeated details](01_reconcile_entries_not_details.ipynb) | Recover entry grain, retain exact decimal signs, and explain why a filtered subset changes the balance comparison. | [Open in Colab](https://colab.research.google.com/github/Query-farm/vgi-iso20022/blob/main/notebooks/01_reconcile_entries_not_details.ipynb) |
| [Compare message representations before status](02_compare_messages_before_status.ipynb) | Inspect MT fields, validate a negative control, preserve both sides of MT103/pacs.008 matching, and join independent pacs.002 observations. | [Open in Colab](https://colab.research.google.com/github/Query-farm/vgi-iso20022/blob/main/notebooks/02_compare_messages_before_status.ipynb) |

Both notebooks have saved results, visible SQL connections, linked worker titles,
Colab buttons and the shared Query Farm Strata Sun/VGI branding. Three SVG figures
stay sharp on GitHub. GitHub displays code expanded; shared setup and small
presentation helpers keep it concise. Editor hiding metadata is only a convenience.

## Run locally or in Colab

Open either Colab link and run all cells. Locally, run from this directory:

```sh
uv sync --locked
uv run --locked vgi-notebooks --timeout 300
```

Select `.venv/bin/python` for an interactive kernel. Standalone setup installs the
shared base package, preserving compatible preinstalled kernel/plot packages.
The local environment adds the execution runner. Authored fixtures and the small
plot helper are fetched from an immutable public revision and SHA-256 verified;
no worker checkout, copied standard or local execution script is required.

The connection remains visible in each notebook:

```sql
ATTACH 'iso20022' (TYPE vgi,
 LOCATION 'github-auto://Query-farm/vgi-iso20022@v0.4.0');
```

The VGI extension downloads the platform's public prebuilt release, checks its
SHA-256 and caches it. Setup needs internet access; subsequent message parsing is
local, with no API credentials, account access or payment submission. The release
supports macOS arm64 and Linux amd64/arm64. No source build or SDK overrides occur.

## Message scope and provenance

All payment messages are synthetic teaching data. The original statement fixtures
have a SHA-256 manifest and an explicit 02 January 2026 period. The migration
fixtures adapt the repository's MIT-licensed golden examples into six authored
identities: three comparable EUR pairs, one currency disagreement, one MT-only
payment and one MX-only payment. They include three independent status reports.
The migration manifest and copied worker license travel with the fixtures.

The first notebook verifies **one statement per file**. Release v0.4.0 predates a
fix that narrows the raw XML for each statement in a multi-statement document;
using those documents with this release could duplicate child rows. Current
source fixes that behavior, but these notebooks deliberately demonstrate the
public release within its verified single-statement scope.

Amounts remain `DECIMAL(38,9)` in SQL and are displayed as strings where exactness
matters. Floating point is used only for chart positioning. Currencies are never
summed together. Repeated statement-detail amounts are recovered by entry identity,
not by deduplicating equal numbers or inventing allocations. Pending entries remain
visible outside the booked-balance identity; reversal flags do not flip the sign twice.

Migration joins require a nonempty unique UETR on each side. Missing counterparts
and currency disagreements remain visible with unavailable deltas. Per-message
validation checks selected structural/field rules, not full XSD/network acceptance.
The changed charge-code control never enters payment comparisons. A matching amount
is not complete semantic equivalence, and a status observation is not independently
verified settlement. Multiple historical status observations would require an explicit
time-selection policy beyond this fixture's one-report-per-UETR constraint.

## Run the experiments separately

Omit small statement entries while keeping the source statements unchanged:

```sh
uv run --locked vgi-notebooks 01_reconcile_entries_not_details.ipynb \
  --parameters '{"MIN_ENTRY_AMOUNT":"100.00"}' --output-dir artifacts/minimum-100
```

The full reconciliation and repeated-detail comparison stay fixed. Selected counts
and filtered residuals change; omitted signed movement explains the residual.
The accepted decimal-string range is 0.00–1000.00. Restore `"0.00"` for the default.

Group a one-cent EUR difference without changing exact equality:

```sh
uv run --locked vgi-notebooks 02_compare_messages_before_status.ipynb \
  --parameters '{"DISPLAY_BAND_EUR":"0.01"}' --output-dir artifacts/one-cent
```

One nonzero difference moves into the display band and one remains outside.
The exact-match count, raw deltas, unmatched/currency populations and status
observations stay fixed. The accepted decimal-string range is 0.00–1.00.
Restore `"0.00"`. The display band is not an accounting or acceptance policy.

The shared runner requires a separate output directory for parameter overrides and
writes notebooks only after successful execution. Defaults, both experiments,
chart reconciliation and one fresh preloaded-kernel bootstrap are checked.
Hosted Colab was not manually operated. Review PNGs and experiment copies are ignored;
the notebook SVG outputs and GitHub SVG assets are committed.

[Coverage evidence](coverage.json) accounts for all eighteen public named SQL
surfaces and verifies ten through focused visible SQL. It separates actual released
execution from reviewed current-source compatibility and documents the multi-statement
behavior difference. Existing green source CI is reused for this notebook-only change.
