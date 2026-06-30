//! Robustness: every public parser must be **total** — arbitrary / malformed
//! bytes may yield empty or partial results but must NEVER panic. This guards the
//! worker's per-row error-capture promise (a bad message can't crash the query).

use iso20022_core::{mt, mx, sniff, validate};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = sniff::detect(&bytes);
        let _ = sniff::mt_type_label(&bytes);
        let _ = validate::validate(&bytes);
        let s = String::from_utf8_lossy(&bytes);
        let _ = mt::mt103::parse(&s);
        let _ = mt::mt202::parse(&s);
        let _ = mt::mt940::parse_file(&s);
        let _ = mt::mt942::parse_file(&s);
        let _ = mt::mt940::parse_lines(&s);
        let _ = mx::pacs008::parse(&s);
        let _ = mx::pacs002::parse(&s);
        let _ = mx::pain001::parse(&s);
        let _ = mx::camt053::parse(&s);
        let _ = mx::camt053::parse_entries(&s);
        let _ = mx::camt054::parse(&s);
        let _ = mx::camt054::parse_entries(&s);
    }

    #[test]
    fn arbitrary_ascii_never_panic(s in "[\\x20-\\x7e:{}\\-\\n,/]{0,400}") {
        let _ = sniff::detect(s.as_bytes());
        let _ = validate::validate(s.as_bytes());
        let _ = mt::mt940::parse_file(&s);
        let _ = mt::block::parse(&s);
    }

    #[test]
    fn malformed_xml_never_panic(s in "<[A-Za-z/ <>\"=0-9]{0,300}") {
        let _ = mx::dom::parse(&s);
        let _ = mx::camt053::parse(&s);
        let _ = validate::validate(s.as_bytes());
    }
}
