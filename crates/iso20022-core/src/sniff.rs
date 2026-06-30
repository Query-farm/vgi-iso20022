//! Message-format detection: SWIFT MT (block-structured FIN) vs ISO 20022 MX
//! (XML), and the specific message type within each family.

/// The detected high-level family + type of a payment message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageKind {
    /// SWIFT MT with its 3-digit type code (e.g. `"103"`, `"202"`, `"940"`).
    Mt(String),
    /// ISO 20022 MX with its message identifier (e.g. `"pacs.008"`, `"camt.053"`).
    Mx(String),
    /// Neither recognized.
    Unknown,
}

/// Detect the family + type of a raw message (bytes may be UTF-8 text or BLOB).
pub fn detect(bytes: &[u8]) -> MessageKind {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n']);
    if trimmed.starts_with('<') {
        if let Some(t) = mx_type(trimmed) {
            return MessageKind::Mx(t);
        }
        return MessageKind::Unknown;
    }
    // MT: starts with a `{1:`/`{2:`/`{4:` block, or a bare `:NN:` tag stream.
    if trimmed.starts_with('{') || trimmed.starts_with(':') || trimmed.contains(":20:") {
        if let Some(t) = mt_type(trimmed) {
            return MessageKind::Mt(t);
        }
        // An MT-shaped message whose type we cannot name is still MT.
        if trimmed.starts_with('{') || trimmed.contains(":20:") {
            return MessageKind::Mt(String::new());
        }
    }
    MessageKind::Unknown
}

/// The MT type code (`"103"`, `"202"`, …) read from the block-2 application
/// header (`{2:I103…}` / `{2:O103…}`), or `None` if absent/unreadable.
pub fn mt_type(msg: &str) -> Option<String> {
    let b2 = block(msg, '2')?;
    // {2:I103BANKDEFFXXXXN} -> first char I/O, then 3-digit type.
    let bytes = b2.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let rest = if matches!(bytes[0], b'I' | b'O') {
        &b2[1..]
    } else {
        &b2[..]
    };
    let code: String = rest.chars().take(3).collect();
    if code.len() == 3 && code.bytes().all(|b| b.is_ascii_digit()) {
        Some(code)
    } else {
        None
    }
}

/// A friendly MT label for `iso20022_mt_type`, e.g. `"MT103"`. Returns `None` for
/// a non-MT (MX) message.
pub fn mt_type_label(bytes: &[u8]) -> Option<String> {
    match detect(bytes) {
        MessageKind::Mt(code) if !code.is_empty() => Some(format!("MT{code}")),
        MessageKind::Mt(_) => Some("MT".to_string()),
        _ => None,
    }
}

/// Extract a top-level SWIFT block body by number: the text between `{N:` and its
/// matching closing brace. Brace-aware so block 3 (`{3:{121:…}}`) and block 4 are
/// handled (block 4 ends at the `-}` trailer, captured up to the closing brace).
pub fn block(msg: &str, n: char) -> Option<String> {
    let needle = format!("{{{n}:");
    let start = msg.find(&needle)? + needle.len();
    let bytes = msg.as_bytes();
    let mut depth = 1usize;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(msg[start..i].to_string());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The MX message identifier (`"pacs.008"`, `"camt.053"`, …) determined from the
/// document's first child local-name, falling back to the namespace URI.
pub fn mx_type(xml: &str) -> Option<String> {
    // Prefer the local-name of the first element under <Document> (or <AppHdr>…).
    if let Some(local) = first_business_element(xml) {
        if let Some(id) = local_to_msg(&local) {
            return Some(id.to_string());
        }
    }
    // Fall back to the xsd namespace: urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08
    if let Some(pos) = xml.find("tech:xsd:") {
        let tail = &xml[pos + "tech:xsd:".len()..];
        let id: String = tail
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '.')
            .collect();
        // Reduce to the family.variant (e.g. pacs.008) prefix.
        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() >= 2 {
            return Some(format!("{}.{}", parts[0], parts[1]));
        }
    }
    None
}

/// Map an MX root business-element local-name to its message id.
fn local_to_msg(local: &str) -> Option<&'static str> {
    Some(match local {
        "FIToFICstmrCdtTrf" => "pacs.008",
        "FIToFIPmtStsRpt" => "pacs.002",
        "CstmrCdtTrfInitn" => "pain.001",
        "BkToCstmrStmt" => "camt.053",
        "BkToCstmrDbtCdtNtfctn" => "camt.054",
        "BkToCstmrAcctRpt" => "camt.052",
        _ => return None,
    })
}

/// Find the local-name of the first business element inside `<Document>` (the one
/// after the Document wrapper), skipping the XML declaration and any `AppHdr`.
fn first_business_element(xml: &str) -> Option<String> {
    let mut rest = xml;
    // Find <Document ...> then the next start tag.
    let doc = rest.find("Document")?;
    rest = &rest[doc..];
    // Skip to the end of the Document open tag.
    let gt = rest.find('>')?;
    rest = &rest[gt + 1..];
    loop {
        let lt = rest.find('<')?;
        rest = &rest[lt + 1..];
        // Skip comments / processing instructions / closing tags.
        if rest.starts_with('!') || rest.starts_with('?') || rest.starts_with('/') {
            let gt = rest.find('>')?;
            rest = &rest[gt + 1..];
            continue;
        }
        // Read the tag name (up to whitespace or > or /), strip a namespace prefix.
        let name: String = rest
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/')
            .collect();
        let local = name.rsplit(':').next().unwrap_or(&name).to_string();
        return Some(local);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MT103: &str =
        "{1:F01BANKBEBBAXXX0000000000}{2:I103BANKDEFFXXXXN}{3:{121:abc}}{4:\n:20:REF\n:32A:260101EUR1234,56\n-}";

    #[test]
    fn detects_mt103() {
        assert_eq!(detect(MT103.as_bytes()), MessageKind::Mt("103".into()));
        assert_eq!(mt_type_label(MT103.as_bytes()).as_deref(), Some("MT103"));
    }

    #[test]
    fn detects_bare_block4() {
        let m = ":20:REF\n:32A:260101EUR10,00\n";
        assert!(matches!(detect(m.as_bytes()), MessageKind::Mt(_)));
    }

    #[test]
    fn detects_mx_by_local_name() {
        let xml = r#"<?xml version="1.0"?><Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr/></FIToFICstmrCdtTrf></Document>"#;
        assert_eq!(detect(xml.as_bytes()), MessageKind::Mx("pacs.008".into()));
    }

    #[test]
    fn detects_mx_camt053() {
        let xml = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08"><BkToCstmrStmt/></Document>"#;
        assert_eq!(detect(xml.as_bytes()), MessageKind::Mx("camt.053".into()));
    }

    #[test]
    fn unknown_is_unknown() {
        assert_eq!(detect(b"hello world"), MessageKind::Unknown);
        assert_eq!(mt_type_label(b"<Document/>"), None);
    }
}
