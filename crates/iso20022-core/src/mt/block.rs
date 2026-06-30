//! SWIFT MT block structure: extract the `{4:…-}` text block and split it into
//! ordered `:NN[a]:` field tags with folded continuation lines.

use crate::sniff;

/// One `:NN[a]:` field occurrence from block 4. `tag` is the bare tag (e.g.
/// `"20"`, `"32A"`, `"50K"`, `"61"`, `"86"`); `value` is its (folded) text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub tag: String,
    pub value: String,
}

/// The parsed field stream of a single MT message's block 4, in document order
/// (repeatable tags such as `:23E:`, `:71F:`, `:61:`, `:86:` are preserved).
#[derive(Debug, Clone, Default)]
pub struct Block4 {
    pub fields: Vec<Tag>,
}

/// Parse a whole MT message into its block-4 field stream. A message may carry
/// block 4 wrapped in `{4:…-}`, or be a bare `:NN:` tag stream (no envelope).
pub fn parse(msg: &str) -> Block4 {
    let body = sniff::block(msg, '4').unwrap_or_else(|| msg.to_string());
    parse_body(&body)
}

/// Parse a raw block-4 body (the text between `{4:` and the closing brace, or a
/// bare tag stream) into ordered fields.
pub fn parse_body(body: &str) -> Block4 {
    let mut fields: Vec<Tag> = Vec::new();
    for raw_line in body.split('\n') {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        // The block-4 terminator.
        if trimmed == "-" || trimmed.is_empty() {
            continue;
        }
        if let Some((tag, value)) = split_tag(line) {
            fields.push(Tag {
                tag,
                value: value.to_string(),
            });
        } else if let Some(last) = fields.last_mut() {
            // Continuation line: fold into the previous field with a newline.
            last.value.push('\n');
            last.value.push_str(line);
        }
    }
    Block4 { fields }
}

/// Split a `:TAG:value` line into `(tag, value)`. Returns `None` if the line is
/// not a tag header (so it is treated as a continuation).
fn split_tag(line: &str) -> Option<(String, &str)> {
    let rest = line.strip_prefix(':')?;
    let close = rest.find(':')?;
    let tag = &rest[..close];
    // A tag is 2 digits + an optional single uppercase letter (e.g. 32A, 50K).
    if tag.len() < 2 || tag.len() > 4 {
        return None;
    }
    let mut chars = tag.chars();
    let ok = chars.next().is_some_and(|c| c.is_ascii_digit())
        && chars.clone().take(1).all(|c| c.is_ascii_digit());
    if !ok {
        return None;
    }
    Some((tag.to_string(), &rest[close + 1..]))
}

impl Block4 {
    /// First value for an exact tag.
    pub fn first(&self, tag: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.tag == tag)
            .map(|f| f.value.as_str())
    }

    /// All values for an exact tag, in order.
    pub fn all(&self, tag: &str) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|f| f.tag == tag)
            .map(|f| f.value.as_str())
            .collect()
    }

    /// First field whose tag starts with `prefix` (option-letter agnostic), e.g.
    /// `prefix = "50"` matches `50A` / `50F` / `50K`. Returns `(full_tag, value)`.
    pub fn first_prefix(&self, prefix: &str) -> Option<(&str, &str)> {
        self.fields
            .iter()
            .find(|f| f.tag.starts_with(prefix))
            .map(|f| (f.tag.as_str(), f.value.as_str()))
    }

    /// Does the message contain any field with this exact tag?
    pub fn has(&self, tag: &str) -> bool {
        self.fields.iter().any(|f| f.tag == tag)
    }

    /// Does the message contain any field whose tag starts with `prefix`?
    pub fn has_prefix(&self, prefix: &str) -> bool {
        self.fields.iter().any(|f| f.tag.starts_with(prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MSG: &str = "{1:F01X}{2:I103Y}{4:\n:20:REF123\n:23E:SDVA\n:23E:INTC\n:50K:/12345678\nACME CORP\n123 MAIN ST\n:59:/DE89370400440532013000\nWIDGETS GMBH\n:71F:EUR5,00\n:71F:EUR3,50\n-}";

    #[test]
    fn folds_continuations() {
        let b4 = parse(MSG);
        assert_eq!(b4.first("20"), Some("REF123"));
        assert_eq!(b4.first("50K"), Some("/12345678\nACME CORP\n123 MAIN ST"));
    }

    #[test]
    fn repeatable_tags_preserved() {
        let b4 = parse(MSG);
        assert_eq!(b4.all("23E"), vec!["SDVA", "INTC"]);
        assert_eq!(b4.all("71F"), vec!["EUR5,00", "EUR3,50"]);
    }

    #[test]
    fn prefix_match() {
        let b4 = parse(MSG);
        assert_eq!(
            b4.first_prefix("50"),
            Some(("50K", "/12345678\nACME CORP\n123 MAIN ST"))
        );
        assert!(b4.has_prefix("59"));
        assert!(!b4.has_prefix("52"));
    }

    #[test]
    fn bare_tag_stream() {
        let b4 = parse(":20:ABC\n:32A:260101EUR1,00\n");
        assert_eq!(b4.first("20"), Some("ABC"));
        assert_eq!(b4.first("32A"), Some("260101EUR1,00"));
    }
}
