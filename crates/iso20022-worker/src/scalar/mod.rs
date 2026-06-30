//! Scalar functions exposed by the iso20022 worker, registered under
//! `iso20022.main`.

mod mt103;
mod mt_type;
mod validate;
mod version;

use vgi::Worker;

/// Register every scalar function on the worker.
pub fn register(worker: &mut Worker) {
    worker.register_scalar(version::Iso20022Version);
    worker.register_scalar(mt_type::MtType);
    worker.register_scalar(mt103::Mt103Field);
    worker.register_scalar(mt103::Mt103Amount);
    worker.register_scalar(validate::Validate);
}
