//! ISO 20022 MX (XML) parsers: a namespace-agnostic DOM and the supported
//! message types (pacs.008, pacs.002, pain.001, camt.053, camt.054).

pub mod camt;
pub mod camt053;
pub mod camt054;
pub mod common;
pub mod dom;
pub mod pacs002;
pub mod pacs008;
pub mod pain001;
