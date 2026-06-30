//! Per-message child table-in-out functions: explode a parent message's `raw`
//! column into child rows (`*_entries` for camt, `*_lines` for MT statements),
//! typically via `LATERAL tf(s.raw)`.

mod common;
mod entries;
mod lines;

use vgi::Worker;

/// Register every table-in-out function on the worker.
pub fn register(worker: &mut Worker) {
    worker.register_table_in_out(entries::camt053_entries());
    worker.register_table_in_out(entries::camt054_entries());
    worker.register_table_in_out(lines::mt940_lines());
    worker.register_table_in_out(lines::mt942_lines());
}
