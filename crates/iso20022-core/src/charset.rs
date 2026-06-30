//! SWIFT FIN character-set validation + sanitization.
//!
//! SWIFT MT restricts text to the **X** (general FIN), **Y** (BIC/ISO), and **Z**
//! (extended) character sets. Disallowed / non-Latin code points (mojibake,
//! control characters) are **sanitized** to a safe placeholder when a field is
//! surfaced as a typed column, while the original bytes are always preserved in
//! the `raw` column — no silent data loss.

/// Replacement code point for a disallowed character (kept ASCII so it never
/// re-introduces a non-Latin byte).
const PLACEHOLDER: char = '.';

/// The SWIFT **X** character set: the general-purpose FIN set used by free-text
/// fields (`:70:`, `:72:`, name & address lines).
///
/// `A-Z a-z 0-9 / - ? : ( ) . , ' + space` plus CR/LF (line structure).
pub fn is_x_char(c: char) -> bool {
    matches!(c,
        'A'..='Z' | 'a'..='z' | '0'..='9' |
        '/' | '-' | '?' | ':' | '(' | ')' | '.' | ',' | '\'' | '+' | ' ' |
        '\r' | '\n'
    )
}

/// Is every character of `s` in the SWIFT X set?
pub fn is_valid_x(s: &str) -> bool {
    s.chars().all(is_x_char)
}

/// Sanitize a field's text for surfacing as a typed column: any character
/// outside the SWIFT X set is replaced by the safe ASCII placeholder. Returns the cleaned
/// string and a flag indicating whether anything was replaced.
pub fn sanitize_x(s: &str) -> (String, bool) {
    let mut changed = false;
    let out: String = s
        .chars()
        .map(|c| {
            if is_x_char(c) {
                c
            } else {
                changed = true;
                PLACEHOLDER
            }
        })
        .collect();
    (out, changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_passes() {
        assert!(is_valid_x("ACME CORP / REF-123 (EUR)"));
    }

    #[test]
    fn mojibake_sanitized() {
        let (clean, changed) = sanitize_x("CAF\u{00c9} \u{00e9}");
        assert!(changed);
        assert_eq!(clean, "CAF. .");
        assert!(!is_valid_x("CAF\u{00c9}"));
    }

    #[test]
    fn control_chars_sanitized() {
        let (clean, changed) = sanitize_x("AB\u{0007}CD");
        assert!(changed);
        assert_eq!(clean, "AB.CD");
    }
}
