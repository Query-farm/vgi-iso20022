//! SWIFT MT (FIN) parsers: block structure, field decomposition, and the four
//! supported message types (MT103, MT202[/COV], MT940, MT942).

pub mod block;
pub mod field;
pub mod mt103;
pub mod mt202;
pub mod mt940;
pub mod mt942;
pub mod uetr;

pub use field::raw_tag;
