//! The externalized scan cursor for the file-glob table functions.
//!
//! `FileGlobCursor` is the **only** scan state — a plain `Serialize`/`Deserialize`
//! struct with no DB handles, file readers, or parsed trees captured. `idx` is
//! the file under the glob; `inner_row` is the next child ordinal within the
//! current file (a multi-statement MT940 or a multi-`CdtTrfTxInf` pacs.008 yields
//! many rows per file). The worker serializes it between batches so DuckDB can
//! suspend / resume / parallelize the scan without dropping or duplicating rows.

use serde::{Deserialize, Serialize};

/// Serializable cursor over a list of resolved paths and the row offset within
/// the current file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileGlobCursor {
    /// All concrete paths the glob/list resolved to, in scan order.
    pub paths: Vec<String>,
    /// Index of the file currently being read.
    pub idx: usize,
    /// Next child-row ordinal to emit within the current file.
    pub inner_row: usize,
}

impl FileGlobCursor {
    /// Create a cursor positioned at the start of `paths`.
    pub fn new(paths: Vec<String>) -> Self {
        FileGlobCursor {
            paths,
            idx: 0,
            inner_row: 0,
        }
    }

    /// Have all files been consumed?
    pub fn is_done(&self) -> bool {
        self.idx >= self.paths.len()
    }

    /// The path currently being read, if any.
    pub fn current(&self) -> Option<&str> {
        self.paths.get(self.idx).map(String::as_str)
    }

    /// Advance to the next file, resetting the inner-row offset.
    pub fn next_file(&mut self) {
        self.idx += 1;
        self.inner_row = 0;
    }

    /// Serialize to compact JSON for the SDK's `encode_resume`.
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Restore from `encode_resume` bytes; a malformed buffer leaves `self` as-is.
    pub fn restore(&mut self, bytes: &[u8]) {
        if let Ok(c) = serde_json::from_slice::<FileGlobCursor>(bytes) {
            *self = c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_mid_file() {
        // Mimic suspending mid-file (a multi-statement MT940): idx=1, inner_row=3.
        let mut c = FileGlobCursor::new(vec![
            "a.txt".to_string(),
            "b.txt".to_string(),
            "c.txt".to_string(),
        ]);
        c.idx = 1;
        c.inner_row = 3;
        let bytes = c.encode();
        let mut restored = FileGlobCursor::default();
        restored.restore(&bytes);
        assert_eq!(c, restored, "cursor must survive an Arrow batch boundary");
    }

    #[test]
    fn malformed_restore_is_noop() {
        let mut c = FileGlobCursor::new(vec!["x".to_string()]);
        let before = c.clone();
        c.restore(b"\x00\x01"); // truncated
        assert_eq!(
            c, before,
            "a malformed resume buffer must not corrupt state"
        );
    }

    #[test]
    fn advances_files() {
        let mut c = FileGlobCursor::new(vec!["a".into(), "b".into()]);
        assert_eq!(c.current(), Some("a"));
        c.inner_row = 5;
        c.next_file();
        assert_eq!(c.current(), Some("b"));
        assert_eq!(c.inner_row, 0);
        c.next_file();
        assert!(c.is_done());
    }
}
