//! Block-3 UETR (`{121:}`) extraction, shared by every MT type.

use crate::sniff;

/// Extract the UETR (unique end-to-end transaction reference) from block 3's
/// `{121:<uuid>}` sub-block, or `None` if absent.
pub fn extract(msg: &str) -> Option<String> {
    let b3 = sniff::block(msg, '3')?;
    let needle = "{121:";
    let pos = b3.find(needle)? + needle.len();
    let tail = &b3[pos..];
    let val: String = tail.chars().take_while(|&c| c != '}').collect();
    let val = val.trim().to_string();
    if val.is_empty() {
        None
    } else {
        Some(val)
    }
}

/// Is `s` a UUID matching the RFC 4122 layout (`8-4-4-4-12` hex)? CBPR+ requires
/// a v4 UETR; the version-nibble check is left to `validate`.
pub fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    let lens = [8usize, 4, 4, 4, 12];
    parts.len() == 5
        && parts
            .iter()
            .zip(lens)
            .all(|(p, n)| p.len() == n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_uetr() {
        let m = "{1:F01X}{3:{108:ABC}{121:e3bf1c2a-1111-4aaa-8bbb-1234567890ab}}{4:\n:20:R\n-}";
        assert_eq!(
            extract(m).as_deref(),
            Some("e3bf1c2a-1111-4aaa-8bbb-1234567890ab")
        );
    }

    #[test]
    fn none_when_absent() {
        assert_eq!(extract("{1:F01X}{4:\n:20:R\n-}"), None);
    }

    #[test]
    fn uuid_shape() {
        assert!(is_uuid("e3bf1c2a-1111-4aaa-8bbb-1234567890ab"));
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("e3bf1c2a11114aaa8bbb1234567890ab"));
    }
}
