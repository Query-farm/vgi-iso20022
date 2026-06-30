//! Library surface of the `iso20022` VGI worker.
//!
//! The binary (`main.rs`) is the actual worker; this `lib` target exposes the
//! Arrow-adapter modules so the in-process tests under each module (and the
//! crate's integration tests) can exercise them without the RPC/IPC plumbing.
//! The pure parse engine lives in the sibling `iso20022-core` crate.

pub mod arrow_io;
pub mod cols;
pub mod meta;
pub mod scalar;
pub mod table;
pub mod table_in_out;

/// Worker version string, surfaced by `iso20022_version()`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
