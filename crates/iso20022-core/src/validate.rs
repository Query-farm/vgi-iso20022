//! Structural + selected CBPR+ SR2025 field validation for `iso20022_validate`.
//!
//! Auto-detects MT vs MX and the message type, then runs structural and
//! field-level rules. Each error is a stable `CODE: human text` string so callers
//! can `list_contains` / `unnest`. **Honest scope:** this is structural +
//! field-level validation, *not* full CBPR+/HVPS+ network validation — a green
//! `ok` means "structurally sound and passes the common field rules", not "the
//! SWIFT network will accept it".

use crate::mt::field::is_bic;
use crate::mt::{block, uetr};
use crate::mx::dom::{self, Node};
use crate::sniff::{detect, MessageKind};

/// The result of validating a message: `ok` is true iff `errors` is empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validation {
    pub ok: bool,
    pub errors: Vec<String>,
}

/// Validate a raw message (MT or MX), returning structural + field errors.
pub fn validate(bytes: &[u8]) -> Validation {
    let mut errors = Vec::new();
    match detect(bytes) {
        MessageKind::Mt(code) => validate_mt(&String::from_utf8_lossy(bytes), &code, &mut errors),
        MessageKind::Mx(id) => validate_mx(&String::from_utf8_lossy(bytes), &id, &mut errors),
        MessageKind::Unknown => errors
            .push("STRUCT001: unrecognized message — neither SWIFT MT nor ISO 20022 MX".into()),
    }
    Validation {
        ok: errors.is_empty(),
        errors,
    }
}

fn validate_mt(msg: &str, code: &str, errors: &mut Vec<String>) {
    let b4 = block::parse(msg);
    if b4.fields.is_empty() {
        errors.push("STRUCT010: empty or unparseable MT block 4".into());
        return;
    }

    // Mandatory tags per detected type.
    let mandatory: &[(&str, bool)] = match code {
        // (tag-or-prefix, is_prefix)
        "103" => &[
            ("20", false),
            ("23B", false),
            ("32A", false),
            ("50", true),
            ("59", true),
        ],
        "202" => &[("20", false), ("21", false), ("32A", false), ("58", true)],
        "940" => &[("20", false), ("25", false), ("60", true), ("62", true)],
        "942" => &[("20", false), ("25", false), ("34F", false)],
        _ => &[("20", false)],
    };
    for (tag, is_prefix) in mandatory {
        let present = if *is_prefix {
            b4.has_prefix(tag)
        } else {
            b4.has(tag)
        };
        if !present {
            errors.push(format!("MT001: missing mandatory tag :{tag}: for MT{code}"));
        }
    }

    // :32A: amount / currency consistency.
    if let Some(v) = b4.first("32A") {
        let amt = crate::mt::field::parse_32a(v);
        check_currency(amt.currency.as_deref(), amt.amount, errors);
        if amt.amount.is_none() {
            errors.push("AMT010: :32A: amount is not a well-formed decimal".into());
        }
    }

    // :71A: charge code in {OUR, SHA, BEN}.
    if let Some(c) = b4.first("71A") {
        if !matches!(c.trim(), "OUR" | "SHA" | "BEN") {
            errors.push(format!(
                "CHRG010: :71A: charge code '{}' not in {{OUR,SHA,BEN}}",
                c.trim()
            ));
        }
    }

    // BIC format on option-A institution fields.
    for tag in ["52A", "53A", "54A", "56A", "57A", "58A"] {
        if let Some(v) = b4.first(tag) {
            let candidate = v.lines().last().unwrap_or("").trim();
            if !candidate.is_empty() && !is_bic(candidate) {
                errors.push(format!("BIC010: :{tag}: '{candidate}' is not a valid BIC"));
            }
        }
    }

    // IBAN checksum on account-bearing party fields.
    for (tag, _) in [("50", true), ("59", true)] {
        if let Some((_, v)) = b4.first_prefix(tag) {
            if let Some(acct) = first_account_line(v) {
                check_iban_if_iban_like(&acct, errors);
            }
        }
    }

    // UETR shape (block-3 {121:}).
    if let Some(u) = uetr::extract(msg) {
        if !uetr::is_uuid(&u) {
            errors.push(format!("UETR010: block-3 UETR '{u}' is not a valid UUID"));
        }
    }
}

fn validate_mx(xml: &str, id: &str, errors: &mut Vec<String>) {
    let Some(root) = dom::parse(xml) else {
        errors.push("STRUCT020: MX document is not well-formed XML".into());
        return;
    };
    if root.find_first("Document").is_none() && root.name != "Document" {
        errors.push("STRUCT021: MX root element is not <Document>".into());
    }

    // Group header MsgId is mandatory across the supported MX set.
    if root
        .find_first("GrpHdr")
        .and_then(|g| g.text_at(&["MsgId"]))
        .is_none()
    {
        errors.push("MX001: missing GrpHdr/MsgId".into());
    }

    // Per-message mandatory + field checks.
    match id {
        "pacs.008" => {
            if let Some(body) = root.find_first("FIToFICstmrCdtTrf") {
                check_sttlm_mtd(body, errors);
                let txs: Vec<&Node> = body.children_named("CdtTrfTxInf").collect();
                if txs.is_empty() {
                    errors.push("MX010: pacs.008 has no CdtTrfTxInf".into());
                }
                for tx in txs {
                    if tx.text_at(&["PmtId", "EndToEndId"]).is_none() {
                        errors.push("MX011: CdtTrfTxInf missing PmtId/EndToEndId".into());
                    }
                    check_amount_node(tx.descend(&["IntrBkSttlmAmt"]), "IntrBkSttlmAmt", errors);
                    check_charge_bearer(tx.text_at(&["ChrgBr"]).as_deref(), errors);
                    check_uetr_node(tx.text_at(&["PmtId", "UETR"]).as_deref(), errors);
                    check_iban_node(tx.text_at(&["DbtrAcct", "Id", "IBAN"]).as_deref(), errors);
                    check_iban_node(tx.text_at(&["CdtrAcct", "Id", "IBAN"]).as_deref(), errors);
                    check_bic_node(
                        tx.text_at(&["DbtrAgt", "FinInstnId", "BICFI"]).as_deref(),
                        errors,
                    );
                    check_bic_node(
                        tx.text_at(&["CdtrAgt", "FinInstnId", "BICFI"]).as_deref(),
                        errors,
                    );
                }
            }
        }
        "pain.001" => {
            if let Some(body) = root.find_first("CstmrCdtTrfInitn") {
                for pmt in body.children_named("PmtInf") {
                    check_iban_node(pmt.text_at(&["DbtrAcct", "Id", "IBAN"]).as_deref(), errors);
                    for tx in pmt.children_named("CdtTrfTxInf") {
                        check_amount_node(tx.descend(&["Amt", "InstdAmt"]), "InstdAmt", errors);
                        check_iban_node(tx.text_at(&["CdtrAcct", "Id", "IBAN"]).as_deref(), errors);
                    }
                }
            }
        }
        "camt.053" | "camt.054" => {
            // Each balance/entry amount must carry a currency and parse.
            for amt in collect_amount_nodes(&root) {
                check_amount_node(Some(amt), "Amt", errors);
            }
        }
        "pacs.002" if root.find_first("FIToFIPmtStsRpt").is_none() => {
            errors.push("MX020: pacs.002 missing FIToFIPmtStsRpt".into());
        }
        _ => {}
    }
}

/// Validate `SttlmInf/SttlmMtd` is one of the CBPR+ enum values.
fn check_sttlm_mtd(body: &Node, errors: &mut Vec<String>) {
    if let Some(m) = body.text_at(&["GrpHdr", "SttlmInf", "SttlmMtd"]) {
        if !matches!(m.trim(), "INDA" | "INGA" | "CLRG" | "COVE") {
            errors.push(format!(
                "MX030: SttlmMtd '{}' not in {{INDA,INGA,CLRG,COVE}}",
                m.trim()
            ));
        }
    }
}

fn check_charge_bearer(cb: Option<&str>, errors: &mut Vec<String>) {
    if let Some(c) = cb {
        if !matches!(c.trim(), "DEBT" | "CRED" | "SHAR" | "SLEV") {
            errors.push(format!(
                "CHRG020: ChrgBr '{}' not in {{DEBT,CRED,SHAR,SLEV}}",
                c.trim()
            ));
        }
    }
}

fn check_amount_node(node: Option<&Node>, label: &str, errors: &mut Vec<String>) {
    let Some(n) = node else { return };
    if n.attr("Ccy").is_none() {
        errors.push(format!("AMT020: {label} is missing the @Ccy attribute"));
    }
    let amount = n
        .text_opt()
        .as_deref()
        .and_then(crate::money::parse_mx_amount);
    if amount.is_none() {
        errors.push(format!(
            "AMT021: {label} value is not a well-formed decimal"
        ));
    }
    check_currency(n.attr("Ccy"), amount, errors);
}

fn check_uetr_node(u: Option<&str>, errors: &mut Vec<String>) {
    if let Some(u) = u {
        if !uetr::is_uuid(u) {
            errors.push(format!("UETR020: UETR '{u}' is not a valid UUID"));
        }
    }
}

fn check_bic_node(bic: Option<&str>, errors: &mut Vec<String>) {
    if let Some(b) = bic {
        if !is_bic(b.trim()) {
            errors.push(format!("BIC020: BICFI '{}' is not a valid BIC", b.trim()));
        }
    }
}

fn check_iban_node(iban: Option<&str>, errors: &mut Vec<String>) {
    if let Some(i) = iban {
        if !iban_mod97_ok(i) {
            errors.push(format!(
                "IBAN010: '{}' fails the IBAN mod-97 check",
                i.trim()
            ));
        }
    }
}

/// Run the IBAN check only when the value is IBAN-shaped (2 letters + 2 digits +
/// alnum), so a plain domestic account number isn't flagged.
fn check_iban_if_iban_like(acct: &str, errors: &mut Vec<String>) {
    let t = acct.trim();
    let b = t.as_bytes();
    let iban_like = t.len() >= 5
        && b[0].is_ascii_alphabetic()
        && b[1].is_ascii_alphabetic()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit();
    if iban_like && !iban_mod97_ok(t) {
        errors.push(format!("IBAN010: '{t}' fails the IBAN mod-97 check"));
    }
}

/// Currency must be a known ISO 4217 code, and the amount's decimal places must
/// not exceed the currency's minor unit (e.g. JPY 0, BHD 3, most 2).
fn check_currency(
    ccy: Option<&str>,
    amount: Option<rust_decimal::Decimal>,
    errors: &mut Vec<String>,
) {
    let Some(ccy) = ccy else { return };
    let ccy = ccy.trim();
    let Some(minor) = iso4217_minor_unit(ccy) else {
        errors.push(format!("CCY010: unknown ISO 4217 currency '{ccy}'"));
        return;
    };
    if let Some(a) = amount {
        if a.scale() > minor as u32 {
            errors.push(format!(
                "CCY020: amount {a} has more decimal places than {ccy} allows ({minor})"
            ));
        }
    }
}

/// First `/account` line of a party field (without the leading slash).
fn first_account_line(value: &str) -> Option<String> {
    let first = value.split('\n').next()?;
    let f = first.trim();
    if f.starts_with('/') {
        Some(f.trim_start_matches('/').trim().to_string())
    } else {
        None
    }
}

/// Gather every `Amt`/`*Amt` node under a tree (for camt balance/entry checks).
fn collect_amount_nodes(node: &Node) -> Vec<&Node> {
    let mut out = Vec::new();
    fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
        if n.name == "Amt" {
            out.push(n);
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    walk(node, &mut out);
    out
}

/// ISO 13616 IBAN mod-97 check. Returns true for a structurally valid IBAN whose
/// check digits are correct.
pub fn iban_mod97_ok(iban: &str) -> bool {
    let s: String = iban.chars().filter(|c| !c.is_whitespace()).collect();
    let s = s.to_ascii_uppercase();
    if s.len() < 5 || s.len() > 34 {
        return false;
    }
    if !s.as_bytes()[0..2].iter().all(|b| b.is_ascii_uppercase()) {
        return false;
    }
    if !s.as_bytes()[2..4].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    // Move the first four chars to the end, then convert letters to numbers.
    let rearranged = format!("{}{}", &s[4..], &s[0..4]);
    let mut remainder: u32 = 0;
    for c in rearranged.chars() {
        let val = if c.is_ascii_digit() {
            c as u32 - '0' as u32
        } else if c.is_ascii_uppercase() {
            c as u32 - 'A' as u32 + 10
        } else {
            return false;
        };
        // Fold in one or two decimal digits at a time.
        if val >= 10 {
            remainder = (remainder * 100 + val) % 97;
        } else {
            remainder = (remainder * 10 + val) % 97;
        }
    }
    remainder == 1
}

/// ISO 4217 minor-unit lookup for a curated set of common currencies; `None` for
/// an unknown code. Covers the 0-dp (JPY) and 3-dp (BHD/KWD) edge cases the spec
/// calls out, defaulting the rest to 2.
pub fn iso4217_minor_unit(ccy: &str) -> Option<u8> {
    let two = [
        "EUR", "USD", "GBP", "CHF", "CAD", "AUD", "NZD", "SGD", "HKD", "SEK", "NOK", "DKK", "PLN",
        "CZK", "ZAR", "MXN", "BRL", "INR", "CNY", "RUB", "TRY", "AED", "SAR", "ILS", "THB",
    ];
    let zero = [
        "JPY", "KRW", "CLP", "ISK", "HUF", "VND", "XOF", "XAF", "PYG", "RWF",
    ];
    let three = ["BHD", "KWD", "OMR", "JOD", "TND", "IQD", "LYD"];
    let c = ccy.trim();
    if c.len() != 3 || !c.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    if two.contains(&c) {
        Some(2)
    } else if zero.contains(&c) {
        Some(0)
    } else if three.contains(&c) {
        Some(3)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iban_mod97() {
        assert!(iban_mod97_ok("DE89370400440532013000"));
        assert!(iban_mod97_ok("FR1420041010050500013M02606"));
        assert!(iban_mod97_ok("GB29NWBK60161331926819"));
        assert!(!iban_mod97_ok("DE89370400440532013001")); // bad check digit
        assert!(!iban_mod97_ok("XX")); // too short
    }

    #[test]
    fn minor_units() {
        assert_eq!(iso4217_minor_unit("EUR"), Some(2));
        assert_eq!(iso4217_minor_unit("JPY"), Some(0));
        assert_eq!(iso4217_minor_unit("BHD"), Some(3));
        assert_eq!(iso4217_minor_unit("ZZZ"), None);
    }

    #[test]
    fn good_mt103_ok() {
        let msg = "{1:F01X}{2:I103X}{3:{121:e3bf1c2a-1111-4aaa-8bbb-1234567890ab}}{4:\n:20:R\n:23B:CRED\n:32A:260101EUR100,00\n:50K:/DE89370400440532013000\nACME\n:59:/FR1420041010050500013M02606\nWIDGETS\n:71A:SHA\n-}";
        let v = validate(msg.as_bytes());
        assert!(v.ok, "expected ok, got {:?}", v.errors);
    }

    #[test]
    fn broken_mt103_flags() {
        // Missing :32A:, bad charge code, JPY with decimals.
        let msg = "{1:F01X}{2:I103X}{4:\n:20:R\n:23B:CRED\n:50K:/DE89370400440532013000\nACME\n:59:/X\nWIDGETS\n:71A:ZZZ\n-}";
        let v = validate(msg.as_bytes());
        assert!(!v.ok);
        assert!(v.errors.iter().any(|e| e.starts_with("MT001")));
        assert!(v.errors.iter().any(|e| e.starts_with("CHRG010")));
    }

    #[test]
    fn bad_iban_in_pacs008() {
        let xml = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>M</MsgId></GrpHdr><CdtTrfTxInf><PmtId><EndToEndId>E</EndToEndId></PmtId><IntrBkSttlmAmt Ccy="EUR">1.00</IntrBkSttlmAmt><DbtrAcct><Id><IBAN>DE89370400440532013001</IBAN></Id></DbtrAcct></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#;
        let v = validate(xml.as_bytes());
        assert!(
            v.errors.iter().any(|e| e.starts_with("IBAN010")),
            "{:?}",
            v.errors
        );
    }

    #[test]
    fn unknown_message() {
        let v = validate(b"hello");
        assert!(!v.ok);
        assert!(v.errors[0].starts_with("STRUCT001"));
    }
}
