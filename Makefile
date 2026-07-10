# vgi-iso20022 worker — dev, test, and lint targets.
#
# Usage:
#   make test         # cargo unit/integration tests + SQL E2E (all transports)
#   make test-unit    # cargo test --workspace (pure-Rust + Arrow-boundary tests)
#   make test-sql     # build the release worker, run the DuckDB sqllogictest suite
#                     #   over every transport (subprocess, http, unix)
#   make lint         # clippy (deny warnings) + rustfmt --check
#   make fmt          # rustfmt the workspace
#   make vgi-lint     # metadata-quality gate (needs uv: `uv tool install` not required)
#
# The SQL E2E suite drives the compiled worker through DuckDB via
# `haybarn-unittest` (install with: `uv tool install haybarn-unittest`).

WORKER         ?= $(CURDIR)/target/release/iso20022-worker
SQL_RUNNER     ?= haybarn-unittest

.PHONY: test test-unit test-sql test-sql-subprocess test-sql-http test-sql-unix lint fmt build doc vgi-lint clean

# Full local gate: everything CI runs (bar the metadata linter, see `vgi-lint`).
test: test-unit test-sql

test-unit:
	cargo test --workspace --all-features

# Build the release worker, then run the SQL E2E suite over every transport.
test-sql: test-sql-subprocess test-sql-http test-sql-unix

test-sql-subprocess: build
	HAYBARN_UNITTEST="$(SQL_RUNNER)" WORKER_BIN="$(WORKER)" TRANSPORT=subprocess ci/run-integration.sh

test-sql-http: build
	HAYBARN_UNITTEST="$(SQL_RUNNER)" WORKER_BIN="$(WORKER)" TRANSPORT=http ci/run-integration.sh

test-sql-unix: build
	HAYBARN_UNITTEST="$(SQL_RUNNER)" WORKER_BIN="$(WORKER)" TRANSPORT=unix ci/run-integration.sh

lint:
	cargo clippy --all-targets --all-features -- -D warnings
	cargo fmt --all -- --check

fmt:
	cargo fmt --all

build:
	cargo build --release --bin iso20022-worker

doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace

# Metadata-quality gate (matches CI's Query-farm/vgi-lint-check@v1 at fail-on=info).
# Unpinned: always resolve the latest published vgi-lint-check, like CI's @v1 action.
vgi-lint: build
	uvx --prerelease=allow --from vgi-lint-check vgi-lint lint "$(WORKER)" --fail-on info --no-check-links

clean:
	cargo clean
