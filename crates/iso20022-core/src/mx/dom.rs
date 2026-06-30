//! A tiny, namespace-agnostic XML DOM built over `quick-xml`.
//!
//! ISO 20022 MX messages are namespaced XML; rather than depend on XSD-generated
//! typed models, the worker walks a minimal tree matched on **local-name** (the
//! element name with any `ns:` prefix stripped) and the `Ccy` attribute. Parsing
//! is total: malformed XML yields `None`, never a panic.

use quick_xml::events::Event;
use quick_xml::Reader;

/// One XML element: its local name, attributes, immediate text, and children.
#[derive(Debug, Clone, Default)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<Node>,
}

/// Parse an XML document into its root [`Node`], or `None` if it is not
/// well-formed enough to walk.
pub fn parse(xml: &str) -> Option<Node> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                stack.push(Node {
                    name: local_name(e.name().as_ref()),
                    attrs: attrs(&e),
                    ..Default::default()
                });
            }
            Ok(Event::Empty(e)) => {
                let node = Node {
                    name: local_name(e.name().as_ref()),
                    attrs: attrs(&e),
                    ..Default::default()
                };
                attach(&mut stack, &mut root, node);
            }
            Ok(Event::End(_)) => {
                if let Some(node) = stack.pop() {
                    attach(&mut stack, &mut root, node);
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(top) = stack.last_mut() {
                    if let Ok(s) = t.unescape() {
                        top.text.push_str(s.trim());
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if let Some(top) = stack.last_mut() {
                    if let Ok(s) = std::str::from_utf8(t.as_ref()) {
                        top.text.push_str(s.trim());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return root, // tolerate a trailing parse error
            _ => {}
        }
    }
    root
}

/// Attach a finished node to its parent, or record it as the root.
fn attach(stack: &mut [Node], root: &mut Option<Node>, node: Node) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else if root.is_none() {
        *root = Some(node);
    }
}

fn local_name(qname: &[u8]) -> String {
    let s = String::from_utf8_lossy(qname);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

fn attrs(e: &quick_xml::events::BytesStart) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for a in e.attributes().flatten() {
        let key = local_name(a.key.as_ref());
        let val = a
            .unescape_value()
            .map(|c| c.into_owned())
            .unwrap_or_default();
        out.push((key, val));
    }
    out
}

impl Node {
    /// First direct child with the given local name.
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    /// All direct children with the given local name.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Follow a path of local names from this node by first-child match.
    pub fn descend(&self, path: &[&str]) -> Option<&Node> {
        let mut cur = self;
        for seg in path {
            cur = cur.child(seg)?;
        }
        Some(cur)
    }

    /// The trimmed, non-empty text at a descendant path.
    pub fn text_at(&self, path: &[&str]) -> Option<String> {
        let n = self.descend(path)?;
        let t = n.text.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    }

    /// This node's own trimmed, non-empty text.
    pub fn text_opt(&self) -> Option<String> {
        let t = self.text.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    }

    /// Attribute value by local name.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// First descendant (any depth, pre-order) with the given local name.
    pub fn find_first(&self, name: &str) -> Option<&Node> {
        if self.name == name {
            return Some(self);
        }
        for c in &self.children {
            if let Some(found) = c.find_first(name) {
                return Some(found);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0"?>
<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08">
  <FIToFICstmrCdtTrf>
    <GrpHdr><MsgId>MSG-1</MsgId><NbOfTxs>2</NbOfTxs></GrpHdr>
    <CdtTrfTxInf>
      <IntrBkSttlmAmt Ccy="EUR">1234.56</IntrBkSttlmAmt>
    </CdtTrfTxInf>
  </FIToFICstmrCdtTrf>
</Document>"#;

    #[test]
    fn parses_and_navigates() {
        let root = parse(XML).unwrap();
        assert_eq!(root.name, "Document");
        let body = root.child("FIToFICstmrCdtTrf").unwrap();
        assert_eq!(body.text_at(&["GrpHdr", "MsgId"]).as_deref(), Some("MSG-1"));
        let tx = body.child("CdtTrfTxInf").unwrap();
        let amt = tx.child("IntrBkSttlmAmt").unwrap();
        assert_eq!(amt.text_opt().as_deref(), Some("1234.56"));
        assert_eq!(amt.attr("Ccy"), Some("EUR"));
    }

    #[test]
    fn malformed_is_none_or_partial() {
        assert!(parse("not xml at all").is_none() || parse("not xml").is_none());
        // A truncated tree returns what was parsed without panicking.
        let _ = parse("<Document><GrpHdr>");
    }
}
