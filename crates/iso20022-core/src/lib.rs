//! Pure-compute parsers for SWIFT **MT** (MT103/202/940/942) and ISO 20022 **MX**
//! (pacs.008/pacs.002/pain.001/camt.053/camt.054) payment messages.
//!
//! This crate has **no Arrow / VGI dependency** and makes **no network calls** —
//! it is the offline parse core behind the `iso20022` VGI worker. Every public
//! entry point is *total*: malformed input yields `None` / an empty result /
//! captured validation errors, **never a panic** (see `tests/fuzz.rs`).
//!
//! Money is parsed with [`rust_decimal::Decimal`] (comma-normalized for MT) so
//! amount equality joins (the MT103 <-> pacs.008 migration-QA use case) are
//! byte-exact with no float drift. Dates use a 1950-2049 century pivot for the
//! `YYMMDD` SWIFT fields and ISO 8601 for MX.
//!
//! ```
//! use iso20022_core::sniff::{detect, MessageKind};
//! let mt = "{1:F01BANKBEBBAXXX0000000000}{2:I103BANKDEFFXXXXN}{4:\n:20:REF\n-}";
//! assert!(matches!(detect(mt.as_bytes()), MessageKind::Mt(_)));
//! ```

pub mod charset;
pub mod cursor;
pub mod dates;
pub mod money;
pub mod mt;
pub mod mx;
pub mod sniff;
pub mod validate;

pub use cursor::FileGlobCursor;
