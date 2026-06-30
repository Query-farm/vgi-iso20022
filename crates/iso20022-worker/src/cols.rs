//! Typed Arrow column builders shared by every table / table-in-out function.
//!
//! Each `*_col` turns a `Vec` of optional Rust values into an `ArrayRef`, and the
//! matching `*_type()` returns the exact `DataType` so `on_bind` can declare a
//! schema that is guaranteed to match what the builder produces (the list/map
//! types are derived from an empty builder so nested field names always agree).

use std::collections::HashMap;
use std::sync::Arc;

use arrow_array::builder::{
    BooleanBuilder, Date32Builder, Decimal128Builder, Int32Builder, ListBuilder, MapBuilder,
    MapFieldNames, StringBuilder, TimestampMicrosecondBuilder,
};
use arrow_array::{Array, ArrayRef};
use arrow_schema::Field;
// Re-exported so table modules using `use crate::cols::*` get `DataType` too.
pub use arrow_schema::DataType;
use chrono::{DateTime, FixedOffset, NaiveDate};
use iso20022_core::dates::{days_since_epoch, micros_since_epoch};
use iso20022_core::money::to_decimal128_i128;
use rust_decimal::Decimal;

/// Money columns are `DECIMAL(38,9)`.
pub const MONEY_PRECISION: u8 = 38;
pub const MONEY_SCALE: i8 = 9;

/// The UTC timestamp type used for every `TIMESTAMPTZ` column.
pub fn timestamp_type() -> DataType {
    DataType::Timestamp(arrow_schema::TimeUnit::Microsecond, Some("UTC".into()))
}

/// The `DECIMAL(38,9)` money type.
pub fn money_type() -> DataType {
    DataType::Decimal128(MONEY_PRECISION, MONEY_SCALE)
}

/// `LIST(VARCHAR)` — derived from an empty builder so the nested item field name
/// matches the builder output exactly.
pub fn list_utf8_type() -> DataType {
    ListBuilder::new(StringBuilder::new())
        .finish()
        .data_type()
        .clone()
}

/// DuckDB names a MAP's nested struct fields `key` / `value` (singular); arrow's
/// default `MapBuilder` uses `keys` / `values`. Use DuckDB's names everywhere so
/// the declared schema matches the built arrays after the DuckDB bind round-trip.
fn map_field_names() -> MapFieldNames {
    MapFieldNames {
        entry: "entries".to_string(),
        key: "key".to_string(),
        value: "value".to_string(),
    }
}

/// `MAP(VARCHAR, VARCHAR)` — derived from an empty builder so the nested entry /
/// key / value field names match the builder output exactly.
pub fn map_utf8_type() -> DataType {
    let mut b = MapBuilder::new(
        Some(map_field_names()),
        StringBuilder::new(),
        StringBuilder::new(),
    );
    b.append(true).unwrap();
    b.finish().data_type().clone()
}

/// A `Field` carrying a column `comment` (surfaced via `duckdb_columns().comment`).
pub fn commented(name: &str, dt: DataType, comment: &str) -> Field {
    Field::new(name, dt, true).with_metadata(HashMap::from([(
        "comment".to_string(),
        comment.to_string(),
    )]))
}

/// Build a VARCHAR column.
pub fn str_col<I, S>(values: I) -> ArrayRef
where
    I: IntoIterator<Item = Option<S>>,
    S: AsRef<str>,
{
    let mut b = StringBuilder::new();
    for v in values {
        match v {
            Some(s) => b.append_value(s.as_ref()),
            None => b.append_null(),
        }
    }
    Arc::new(b.finish())
}

/// Build a `DECIMAL(38,9)` column from `Decimal` values (clamped to the scale).
pub fn dec_col<I: IntoIterator<Item = Option<Decimal>>>(values: I) -> ArrayRef {
    let mut b = Decimal128Builder::new();
    for v in values {
        match v.and_then(to_decimal128_i128) {
            Some(m) => b.append_value(m),
            None => b.append_null(),
        }
    }
    Arc::new(
        b.finish()
            .with_precision_and_scale(MONEY_PRECISION, MONEY_SCALE)
            .expect("DECIMAL(38,9)"),
    )
}

/// Build a `DATE` (Date32) column.
pub fn date_col<I: IntoIterator<Item = Option<NaiveDate>>>(values: I) -> ArrayRef {
    let mut b = Date32Builder::new();
    for v in values {
        match v.and_then(days_since_epoch) {
            Some(d) => b.append_value(d),
            None => b.append_null(),
        }
    }
    Arc::new(b.finish())
}

/// Build a `TIMESTAMPTZ` (Timestamp µs, UTC) column.
pub fn ts_col<I: IntoIterator<Item = Option<DateTime<FixedOffset>>>>(values: I) -> ArrayRef {
    let mut b = TimestampMicrosecondBuilder::new();
    for v in values {
        match v {
            Some(dt) => b.append_value(micros_since_epoch(dt)),
            None => b.append_null(),
        }
    }
    Arc::new(b.finish().with_timezone("UTC"))
}

/// Build a `BOOLEAN` column.
pub fn bool_col<I: IntoIterator<Item = Option<bool>>>(values: I) -> ArrayRef {
    let mut b = BooleanBuilder::new();
    for v in values {
        match v {
            Some(x) => b.append_value(x),
            None => b.append_null(),
        }
    }
    Arc::new(b.finish())
}

/// Build an `INTEGER` (Int32) column.
pub fn int_col<I: IntoIterator<Item = Option<i32>>>(values: I) -> ArrayRef {
    let mut b = Int32Builder::new();
    for v in values {
        match v {
            Some(x) => b.append_value(x),
            None => b.append_null(),
        }
    }
    Arc::new(b.finish())
}

/// Build a `LIST(VARCHAR)` column (one inner list per row; never null at the
/// list level — an empty `Vec` becomes an empty list).
pub fn list_str_col<I, R, S>(rows: I) -> ArrayRef
where
    I: IntoIterator<Item = R>,
    R: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut b = ListBuilder::new(StringBuilder::new());
    for row in rows {
        for item in row {
            b.values().append_value(item.as_ref());
        }
        b.append(true);
    }
    Arc::new(b.finish())
}

/// Build a `MAP(VARCHAR, VARCHAR)` column (one map per row).
pub fn map_str_col<I, R>(rows: I) -> ArrayRef
where
    I: IntoIterator<Item = R>,
    R: IntoIterator<Item = (String, String)>,
{
    let mut b = MapBuilder::new(
        Some(map_field_names()),
        StringBuilder::new(),
        StringBuilder::new(),
    );
    for row in rows {
        for (k, v) in row {
            b.keys().append_value(k);
            b.values().append_value(v);
        }
        b.append(true).expect("map row");
    }
    Arc::new(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::Array;

    #[test]
    fn decimal_scale_is_nine() {
        let arr = dec_col([Some(Decimal::new(123456, 2)), None]);
        assert_eq!(arr.data_type(), &money_type());
        assert_eq!(arr.len(), 2);
        assert!(arr.is_null(1));
    }

    #[test]
    fn list_and_map_types_match_builders() {
        let l = list_str_col([vec!["a", "b"], vec![]]);
        assert_eq!(l.data_type(), &list_utf8_type());
        let m = map_str_col([vec![("k".to_string(), "v".to_string())]]);
        assert_eq!(m.data_type(), &map_utf8_type());
    }

    #[test]
    fn timestamp_carries_utc() {
        let arr = ts_col([None]);
        assert_eq!(arr.data_type(), &timestamp_type());
    }
}
