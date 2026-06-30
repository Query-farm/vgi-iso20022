//! Shared MX field extraction helpers over the `Node` DOM tree.

use super::dom::Node;
use crate::dates::{parse_iso_date, parse_iso_datetime};
use crate::money::parse_mx_amount;
use chrono::{DateTime, FixedOffset, NaiveDate};
use rust_decimal::Decimal;

/// A money element: `<…Amt Ccy="CCY">value</…Amt>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Money {
    pub amount: Option<Decimal>,
    pub ccy: Option<String>,
}

/// Read a money element at `path` from `node`.
pub fn money_at(node: &Node, path: &[&str]) -> Money {
    match node.descend(path) {
        Some(n) => Money {
            amount: n.text_opt().as_deref().and_then(parse_mx_amount),
            ccy: n.attr("Ccy").map(|s| s.to_string()),
        },
        None => Money::default(),
    }
}

/// Read a decimal (rate / control-sum) text element at `path`.
pub fn decimal_at(node: &Node, path: &[&str]) -> Option<Decimal> {
    node.text_at(path).as_deref().and_then(parse_mx_amount)
}

/// Read an `ISODate` (or the date part of an `ISODateTime`) at `path`.
pub fn date_at(node: &Node, path: &[&str]) -> Option<NaiveDate> {
    node.text_at(path).as_deref().and_then(parse_iso_date)
}

/// Read an `ISODateTime` at `path`.
pub fn datetime_at(node: &Node, path: &[&str]) -> Option<DateTime<FixedOffset>> {
    node.text_at(path).as_deref().and_then(parse_iso_datetime)
}

/// Read an integer text element at `path`.
pub fn int_at(node: &Node, path: &[&str]) -> Option<i32> {
    node.text_at(path)?.trim().parse::<i32>().ok()
}

/// Collect the text of every direct/nested repeat of an element at `parent`'s
/// `path`'s parent — used for `RmtInf/Ustrd` repeats. `path` is the parent
/// container; `leaf` the repeating child.
pub fn collect_texts(node: &Node, container: &[&str], leaf: &str) -> Vec<String> {
    match node.descend(container) {
        Some(c) => c
            .children_named(leaf)
            .filter_map(|n| n.text_opt())
            .collect(),
        None => Vec::new(),
    }
}
