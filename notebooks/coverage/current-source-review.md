# Release and current-source comparison

The saved exports are from the actual public v0.4.0 binary through Haybarn.
They contain seventeen functions and one view, eighteen logical names. Four
scalars, nine file readers, four relation-input exploders and supported_messages
are registered in current source3a4536bf. The README's iso20022_version row is
stale: this name is absent from both actual release and current registration.
No authentication, write or COPY objects are excluded from the denominator.

Current source was reviewed, not built or executed for these notebooks. Full
release-to-current source/dependency diff and exact registration snapshots are
included. Function argument/return schemas and the supported-message relation
are unchanged. Additions to example_queries and composed tasks are metadata.

There IS a behavior change: current camt053_read and camt054_read narrow the raw
XML document to the particular statement/notification. Releasev0.4.0 repeats the
whole document for each parent row, so exploding files with multiple statements
can duplicate child rows. The first notebook explicitly verifies one statement
per file before launching the release. The changed source keeps single-statement
documents byte-for-byte (its added unit assertion), preserving that example's
scope; no multi-statement compatibility is claimed. The migration notebook uses
unchanged MT103/pacs008/pacs002 parsers. Root dependencies are unchanged.

Green source CI43572293353 at3a4536bf is reused for notebook-only changes.
The saved runtime receipts prove only releasev0.4.0 execution. Current coverage
is source-compatible coverage for these constrained examples, not current runtime.
