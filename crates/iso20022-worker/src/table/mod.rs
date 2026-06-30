//! File-glob table functions exposed by the iso20022 worker, registered under
//! `iso20022.main`: the nine `*_read` scans.

mod camt053;
mod camt054;
mod common;
mod mt103;
mod mt202;
mod mt940;
mod mt942;
mod pacs002;
mod pacs008;
mod pain001;
mod scan;

use vgi::Worker;

/// Register every file-glob table function on the worker.
pub fn register(worker: &mut Worker) {
    worker.register_table(mt103::table());
    worker.register_table(mt202::table());
    worker.register_table(mt940::table());
    worker.register_table(mt942::table());
    worker.register_table(pacs008::table());
    worker.register_table(pacs002::table());
    worker.register_table(pain001::table());
    worker.register_table(camt053::table());
    worker.register_table(camt054::table());
}
