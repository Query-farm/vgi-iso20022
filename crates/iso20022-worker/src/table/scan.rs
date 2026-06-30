//! Shared file-glob scan machinery for the `*_read` table functions.
//!
//! A `*_read` function resolves its glob/path argument to a [`FileGlobCursor`]
//! of concrete paths, then streams one file's rows per `next_batch`. The cursor
//! is the only state and is externalized via `encode_resume` / `restore_resume`
//! so DuckDB can suspend / resume / parallelize the scan. A file that is missing
//! or fails to parse is skipped (its row count is simply zero) — a malformed file
//! never aborts the query.

use arrow_array::RecordBatch;
use arrow_schema::SchemaRef;
use iso20022_core::FileGlobCursor;
use vgi::arguments::Arguments;
use vgi::table_function::TableProducer;
use vgi_rpc::{OutputCollector, Result, RpcError};

/// Build a batch for a single file's `(path, content)`; returns rows for that
/// file (possibly empty if it does not parse).
pub type BuildFn = fn(&str, &str) -> RecordBatch;

/// Resolve the positional path/glob argument (position 0) to concrete file paths
/// in sorted order. A plain path is returned as-is; a glob is expanded.
pub fn resolve_paths(args: &Arguments) -> Result<Vec<String>> {
    let arg = args
        .const_str(0)
        .ok_or_else(|| RpcError::value_error("a file path or glob argument is required"))?;
    Ok(expand(&arg))
}

/// Expand a single path/glob into concrete paths (sorted). Non-glob inputs are
/// returned verbatim so a literal filename still scans.
pub fn expand(pattern: &str) -> Vec<String> {
    if pattern.contains(['*', '?', '[']) {
        match glob::glob(pattern) {
            Ok(paths) => {
                let mut out: Vec<String> = paths
                    .flatten()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                out.sort();
                out
            }
            Err(_) => Vec::new(),
        }
    } else {
        vec![pattern.to_string()]
    }
}

/// The streaming producer: one file -> one batch, advancing the cursor. The
/// declared schema lives in each module's `schema()` and is rebuilt inside the
/// `build` fn, so the producer only needs the cursor and the build pointer.
pub struct GlobProducer {
    cursor: FileGlobCursor,
    build: BuildFn,
}

impl GlobProducer {
    pub fn new(_schema: SchemaRef, paths: Vec<String>, build: BuildFn) -> Self {
        GlobProducer {
            cursor: FileGlobCursor::new(paths),
            build,
        }
    }
}

impl TableProducer for GlobProducer {
    fn next_batch(&mut self, _out: &mut OutputCollector) -> Result<Option<RecordBatch>> {
        while let Some(path) = self.cursor.current().map(str::to_string) {
            self.cursor.next_file();
            let Ok(bytes) = std::fs::read(&path) else {
                continue; // unreadable file -> skip
            };
            let content = String::from_utf8_lossy(&bytes).into_owned();
            let batch = (self.build)(&path, &content);
            if batch.num_rows() > 0 {
                // Re-key the batch onto our declared schema (identical layout).
                return Ok(Some(batch));
            }
        }
        Ok(None)
    }

    fn encode_resume(&self) -> Vec<u8> {
        self.cursor.encode()
    }

    fn restore_resume(&mut self, bytes: &[u8]) {
        self.cursor.restore(bytes);
    }

    fn resume_supported(&self) -> bool {
        true
    }
}

/// Helper used by the build fns: the `schema` is needed to construct the batch.
/// We keep schema construction in each module's `schema()` and pass the columns.
pub fn finish(schema: &SchemaRef, columns: Vec<arrow_array::ArrayRef>) -> RecordBatch {
    RecordBatch::try_new(schema.clone(), columns).expect("column/schema layout mismatch")
}
