//! MT940 — customer statement (`mt940_read` headers + `mt940_lines` lines).
//!
//! A file may hold **multiple** statements; [`parse_file`] returns one
//! [`Statement`] per `:20:`, each with a 0-based `statement_idx`. Statement lines
//! (`:61:` joined to a following `:86:`) are parsed by [`parse_lines`], which the
//! worker also exposes per-blob as `mt940_lines(raw)`.

use super::block::{self, Block4, Tag};
use crate::charset;
use crate::dates::{parse_mmdd, parse_yymmdd};
use crate::money::parse_mt_amount;
use crate::sniff;
use chrono::NaiveDate;
use rust_decimal::Decimal;

/// A single MT940 statement (header fields + parsed lines).
#[derive(Debug, Clone, Default)]
pub struct Statement {
    pub statement_idx: i32,
    pub transaction_ref: Option<String>,
    pub related_ref: Option<String>,
    pub account: Option<String>,
    pub statement_no: Option<String>,
    pub sequence_no: Option<String>,
    pub opening_balance: Option<Decimal>,
    pub opening_balance_dc: Option<String>,
    pub opening_balance_date: Option<NaiveDate>,
    pub opening_is_intermediate: bool,
    pub ccy: Option<String>,
    pub closing_balance: Option<Decimal>,
    pub closing_balance_dc: Option<String>,
    pub closing_balance_date: Option<NaiveDate>,
    pub closing_available: Option<Decimal>,
    pub forward_available: Option<Decimal>,
    pub line_count: i32,
    /// Reconstructed text of just this statement (carried as the `raw` column).
    pub raw: String,
    pub lines: Vec<Line>,
}

/// One MT940 `:61:` statement line, joined to its following `:86:` narrative.
#[derive(Debug, Clone, Default)]
pub struct Line {
    pub line_idx: i32,
    pub value_date: Option<NaiveDate>,
    pub entry_date: Option<NaiveDate>,
    pub credit_debit: Option<String>,
    pub funds_code: Option<String>,
    pub amount: Option<Decimal>,
    pub transaction_type_id: Option<String>,
    pub customer_ref: Option<String>,
    pub bank_ref: Option<String>,
    pub supplementary: Option<String>,
    pub narrative: Option<String>,
    /// Structured `:86:` subfields (`?NN` / `>NN`), empty when unstructured.
    pub narrative_struct: Vec<(String, String)>,
}

/// A parsed `:60F/M:` / `:62F/M:` balance.
#[derive(Debug, Clone, Default)]
pub struct Balance {
    pub dc: Option<String>,
    pub date: Option<NaiveDate>,
    pub ccy: Option<String>,
    /// Signed amount (debit balances are negative).
    pub amount: Option<Decimal>,
}

/// Parse a `:60a:`/`:62a:`/`:64:`/`:65:` balance value:
/// `<D|C><YYMMDD><CCY><amount>`.
pub fn parse_balance(value: &str) -> Balance {
    let v = value.trim();
    let mut b = Balance::default();
    let bytes = v.as_bytes();
    if bytes.is_empty() {
        return b;
    }
    let mut pos = 0;
    let dc = match bytes[0] {
        b'C' => Some("C"),
        b'D' => Some("D"),
        _ => None,
    };
    if dc.is_some() {
        pos = 1;
    }
    b.dc = dc.map(|s| s.to_string());
    b.date = v.get(pos..pos + 6).and_then(parse_yymmdd);
    if v.len() >= pos + 6 {
        pos += 6;
    }
    b.ccy = v.get(pos..pos + 3).map(|s| s.to_string());
    if v.len() >= pos + 3 {
        pos += 3;
    }
    let amount = v.get(pos..).and_then(parse_mt_amount);
    b.amount = amount.map(|a| if dc == Some("D") { -a } else { a });
    b
}

/// Parse the whole file content into statements (one per `:20:`).
pub fn parse_file(content: &str) -> Vec<Statement> {
    let mut out = Vec::new();
    for body in block4_bodies(content) {
        let b4 = block::parse_body(&body);
        for group in split_statements(&b4.fields) {
            let mut st = statement_from_fields(&group);
            st.statement_idx = out.len() as i32;
            out.push(st);
        }
    }
    out
}

/// Reconstruct the `:tag:value` text of a field group (used as the `raw` column).
pub(crate) fn serialize_fields(fields: &[Tag]) -> String {
    let mut s = String::new();
    for f in fields {
        s.push(':');
        s.push_str(&f.tag);
        s.push(':');
        s.push_str(&f.value);
        s.push('\n');
    }
    s
}

/// Build a [`Statement`] from one field group (header + its `:61:`/`:86:` lines).
fn statement_from_fields(fields: &[Tag]) -> Statement {
    let b4 = Block4 {
        fields: fields.to_vec(),
    };
    let lines = parse_lines_from(&b4);
    let (stmt_no, seq_no) = split_28c(b4.first("28C"));

    let opening = b4
        .first("60F")
        .map(|v| (false, parse_balance(v)))
        .or_else(|| b4.first("60M").map(|v| (true, parse_balance(v))));
    let closing = b4
        .first("62F")
        .map(parse_balance)
        .or_else(|| b4.first("62M").map(parse_balance));
    let (opening_is_intermediate, opening_bal) = match opening {
        Some((interm, b)) => (interm, Some(b)),
        None => (false, None),
    };

    let ccy = opening_bal
        .as_ref()
        .and_then(|b| b.ccy.clone())
        .or_else(|| closing.as_ref().and_then(|b| b.ccy.clone()));

    Statement {
        statement_idx: 0,
        transaction_ref: b4.first("20").map(san),
        related_ref: b4.first("21").map(san),
        account: b4.first("25").map(san),
        statement_no: stmt_no,
        sequence_no: seq_no,
        opening_balance: opening_bal.as_ref().and_then(|b| b.amount),
        opening_balance_dc: opening_bal.as_ref().and_then(|b| b.dc.clone()),
        opening_balance_date: opening_bal.as_ref().and_then(|b| b.date),
        opening_is_intermediate,
        ccy,
        closing_balance: closing.as_ref().and_then(|b| b.amount),
        closing_balance_dc: closing.as_ref().and_then(|b| b.dc.clone()),
        closing_balance_date: closing.as_ref().and_then(|b| b.date),
        closing_available: b4.first("64").and_then(|v| parse_balance(v).amount),
        forward_available: b4.first("65").and_then(|v| parse_balance(v).amount),
        line_count: lines.len() as i32,
        raw: serialize_fields(fields),
        lines,
    }
}

/// Public per-blob entry point: parse `:61:`/`:86:` lines from a statement blob.
pub fn parse_lines(blob: &str) -> Vec<Line> {
    let body = sniff::block(blob, '4').unwrap_or_else(|| blob.to_string());
    let b4 = block::parse_body(&body);
    parse_lines_from(&b4)
}

/// Parse `:61:` lines (each joined to a following `:86:`) from a field stream.
pub(crate) fn parse_lines_from(b4: &Block4) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    let mut idx = 0i32;
    for f in &b4.fields {
        match f.tag.as_str() {
            "61" => {
                let mut line = parse_61(&f.value);
                line.line_idx = idx;
                idx += 1;
                lines.push(line);
            }
            "86" => {
                if let Some(last) = lines.last_mut() {
                    apply_86(last, &f.value);
                }
            }
            _ => {}
        }
    }
    lines
}

/// Parse a `:61:` line value into its subfields.
fn parse_61(value: &str) -> Line {
    let mut line = Line::default();
    // Split off the supplementary (subfield 7) — the part after the first
    // newline within the folded :61: value.
    let (head, supplementary) = match value.split_once('\n') {
        Some((h, s)) => (h.trim_end_matches('\r'), Some(s.replace('\r', ""))),
        None => (value, None),
    };
    line.supplementary = supplementary.filter(|s| !s.trim().is_empty());

    let bytes = head.as_bytes();
    let mut pos = 0usize;

    // Subfield 1: value date YYMMDD.
    line.value_date = head.get(0..6).and_then(parse_yymmdd);
    pos += 6.min(bytes.len());

    // Subfield 2: optional entry date MMDD (only if the next 4 are digits).
    if head
        .get(pos..pos + 4)
        .is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit()))
    {
        line.entry_date = head
            .get(pos..pos + 4)
            .and_then(|s| parse_mmdd(s, line.value_date));
        pos += 4;
    }

    // Subfield 3: D/C mark (C, D, RC, RD).
    let rest = &head[pos.min(head.len())..];
    let (dc, consumed) = if rest.starts_with("RC") {
        ("RC", 2)
    } else if rest.starts_with("RD") {
        ("RD", 2)
    } else if rest.starts_with('C') {
        ("C", 1)
    } else if rest.starts_with('D') {
        ("D", 1)
    } else {
        ("", 0)
    };
    if !dc.is_empty() {
        line.credit_debit = Some(dc.to_string());
    }
    pos += consumed;

    // Subfield 3b: optional one-char funds code (a letter before the amount).
    if let Some(c) = head[pos.min(head.len())..].chars().next() {
        if c.is_ascii_alphabetic() {
            line.funds_code = Some(c.to_string());
            pos += c.len_utf8();
        }
    }

    // Subfield 4: amount — digits + comma until the transaction-type letter.
    let amount_str: String = head[pos.min(head.len())..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .collect();
    pos += amount_str.len();
    line.amount = parse_mt_amount(&amount_str);

    // Subfield 5: transaction type id — one of N/S/F + 3 chars.
    let after = &head[pos.min(head.len())..];
    if after.len() >= 4 && matches!(after.as_bytes()[0], b'N' | b'S' | b'F') {
        line.transaction_type_id = Some(after[0..4].to_string());
        pos += 4;
    }

    // Subfield 6: customer ref [//bank ref].
    let refs = &head[pos.min(head.len())..];
    if !refs.is_empty() {
        match refs.split_once("//") {
            Some((cust, bank)) => {
                line.customer_ref = non_empty(cust);
                line.bank_ref = non_empty(bank);
            }
            None => line.customer_ref = non_empty(refs),
        }
    }
    line
}

/// Apply a `:86:` narrative to the most recent line: fold the free text and, when
/// structured (`?NN` / `>NN` markers), populate `narrative_struct`.
fn apply_86(line: &mut Line, value: &str) {
    let (clean, _) = charset::sanitize_x(value);
    line.narrative = Some(clean.clone());
    line.narrative_struct = parse_structured_narrative(&clean);
}

/// Split a structured `:86:` narrative on `?NN` / `>NN` markers into `(key, val)`
/// pairs. Returns empty for unstructured (free-text) narratives.
fn parse_structured_narrative(text: &str) -> Vec<(String, String)> {
    let trimmed = text.trim();
    let marker = if trimmed.starts_with('?') {
        '?'
    } else if trimmed.starts_with('>') {
        '>'
    } else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // Skip a leading GVC code for '>' German format is handled by treating the
    // whole thing as ?NN/>NN segments.
    for seg in trimmed.split(marker).filter(|s| !s.is_empty()) {
        if seg.len() >= 2 && seg[0..2].bytes().all(|b| b.is_ascii_digit()) {
            let key = seg[0..2].to_string();
            let val = seg[2..].trim().to_string();
            out.push((key, val));
        }
    }
    out
}

/// Split `:28C:` into `(statement_no, sequence_no)` on `/`.
fn split_28c(value: Option<&str>) -> (Option<String>, Option<String>) {
    match value {
        None => (None, None),
        Some(v) => match v.split_once('/') {
            Some((n, s)) => (non_empty(n), non_empty(s)),
            None => (non_empty(v), None),
        },
    }
}

/// Split the field stream into per-statement groups, each starting at a `:20:`.
pub(crate) fn split_statements(fields: &[Tag]) -> Vec<Vec<Tag>> {
    let mut groups: Vec<Vec<Tag>> = Vec::new();
    for f in fields {
        if f.tag == "20" || groups.is_empty() {
            groups.push(Vec::new());
        }
        if let Some(g) = groups.last_mut() {
            g.push(f.clone());
        }
    }
    groups.into_iter().filter(|g| !g.is_empty()).collect()
}

/// All `{4:…}` block bodies in a file, or the whole content if there is no
/// envelope (a bare tag stream).
pub(crate) fn block4_bodies(content: &str) -> Vec<String> {
    let mut bodies = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find("{4:") {
        let after = &rest[start..];
        if let Some(body) = sniff::block(after, '4') {
            bodies.push(body);
            // Advance past this block.
            let consumed = after.find('}').map(|i| start + i + 1).unwrap_or(rest.len());
            rest = &rest[consumed.min(rest.len())..];
        } else {
            break;
        }
    }
    if bodies.is_empty() {
        bodies.push(content.to_string());
    }
    bodies
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn san(s: &str) -> String {
    charset::sanitize_x(s).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const STMT: &str = "{1:F01X}{2:O940X}{4:\n:20:STMT-1\n:25:DE89370400440532013000\n:28C:12345/1\n:60F:C260101EUR1000,00\n:61:2601020102C500,00NTRFNONREF//BANK-A\nEXTRA INFO\n:86:GROCERY STORE PAYMENT\n:61:2601030103D250,50NMSCCUST-REF//BANK-B\n:86:?20RENT ?30BANK ?31ACC123\n:62F:C260103EUR1249,50\n:64:C260103EUR1249,50\n-}";

    #[test]
    fn statement_header() {
        let sts = parse_file(STMT);
        assert_eq!(sts.len(), 1);
        let s = &sts[0];
        assert_eq!(s.transaction_ref.as_deref(), Some("STMT-1"));
        assert_eq!(s.account.as_deref(), Some("DE89370400440532013000"));
        assert_eq!(s.statement_no.as_deref(), Some("12345"));
        assert_eq!(s.sequence_no.as_deref(), Some("1"));
        assert_eq!(
            s.opening_balance,
            Some(Decimal::from_str("1000.00").unwrap())
        );
        assert_eq!(s.opening_balance_dc.as_deref(), Some("C"));
        assert_eq!(s.ccy.as_deref(), Some("EUR"));
        assert_eq!(
            s.closing_balance,
            Some(Decimal::from_str("1249.50").unwrap())
        );
        assert_eq!(s.line_count, 2);
    }

    #[test]
    fn line_subfields() {
        let sts = parse_file(STMT);
        let l0 = &sts[0].lines[0];
        assert_eq!(l0.value_date, NaiveDate::from_ymd_opt(2026, 1, 2));
        assert_eq!(l0.entry_date, NaiveDate::from_ymd_opt(2026, 1, 2));
        assert_eq!(l0.credit_debit.as_deref(), Some("C"));
        assert_eq!(l0.amount, Some(Decimal::from_str("500.00").unwrap()));
        assert_eq!(l0.transaction_type_id.as_deref(), Some("NTRF"));
        assert_eq!(l0.customer_ref.as_deref(), Some("NONREF"));
        assert_eq!(l0.bank_ref.as_deref(), Some("BANK-A"));
        assert_eq!(l0.supplementary.as_deref(), Some("EXTRA INFO"));
        assert_eq!(l0.narrative.as_deref(), Some("GROCERY STORE PAYMENT"));
        assert!(l0.narrative_struct.is_empty());
    }

    #[test]
    fn structured_narrative_and_debit() {
        let sts = parse_file(STMT);
        let l1 = &sts[0].lines[1];
        assert_eq!(l1.credit_debit.as_deref(), Some("D"));
        assert_eq!(l1.amount, Some(Decimal::from_str("250.50").unwrap()));
        assert_eq!(l1.transaction_type_id.as_deref(), Some("NMSC"));
        assert_eq!(
            l1.narrative_struct,
            vec![
                ("20".to_string(), "RENT".to_string()),
                ("30".to_string(), "BANK".to_string()),
                ("31".to_string(), "ACC123".to_string()),
            ]
        );
    }

    #[test]
    fn multi_statement_file() {
        let two = format!("{STMT}\n{}", STMT.replace("STMT-1", "STMT-2"));
        let sts = parse_file(&two);
        assert_eq!(sts.len(), 2);
        assert_eq!(sts[0].statement_idx, 0);
        assert_eq!(sts[1].statement_idx, 1);
        assert_eq!(sts[1].transaction_ref.as_deref(), Some("STMT-2"));
    }

    #[test]
    fn lines_from_blob() {
        let sts = parse_file(STMT);
        let raw = &sts[0].raw;
        let lines = parse_lines(raw);
        assert_eq!(lines.len(), 2);
    }
}
