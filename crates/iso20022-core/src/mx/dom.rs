//! A tiny, namespace-agnostic XML DOM built over `quick-xml`.
//!
//! ISO 20022 MX messages are namespaced XML; rather than depend on XSD-generated
//! typed models, the worker walks a minimal tree matched on **local-name** (the
//! element name with any `ns:` prefix stripped) and the `Ccy` attribute. Parsing
//! is total: malformed XML yields `None`, never a panic.

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

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
    // quick-xml (>= 0.38) splits character data around entity references
    // (`A &amp; B` arrives as Text, GeneralRef, Text), so a contiguous text run
    // is accumulated here and flushed -- trimmed, as one unit -- at the next
    // markup event. `None` marks a run holding an unknown entity, which is
    // dropped like the whole-text unescape failure it replaces.
    let mut run: Option<String> = Some(String::new());

    loop {
        let event = reader.read_event();
        if !matches!(event, Ok(Event::Text(_)) | Ok(Event::GeneralRef(_))) {
            flush_text(&mut stack, &mut run);
        }
        match event {
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
                if let Some(buf) = run.as_mut() {
                    buf.push_str(&t.xml10_content());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                let resolved = match r.resolve_char_ref() {
                    Ok(Some(c)) => Some(c.to_string()),
                    Ok(None) => resolve_predefined_entity(&r).map(str::to_string),
                    Err(_) => None,
                };
                match (run.as_mut(), resolved) {
                    (Some(buf), Some(s)) => buf.push_str(&s),
                    _ => run = None,
                }
            }
            Ok(Event::CData(t)) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(t.xml10_content().trim());
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return root, // tolerate a trailing parse error
            _ => {}
        }
    }
    root
}

/// Append the pending text run (trimmed) to the open element, then reset it.
fn flush_text(stack: &mut [Node], run: &mut Option<String>) {
    if let (Some(top), Some(buf)) = (stack.last_mut(), run.as_ref()) {
        top.text.push_str(buf.trim());
    }
    *run = Some(String::new());
}

/// Attach a finished node to its parent, or record it as the root.
fn attach(stack: &mut [Node], root: &mut Option<Node>, node: Node) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else if root.is_none() {
        *root = Some(node);
    }
}

fn local_name(qname: &str) -> String {
    qname.rsplit(':').next().unwrap_or(qname).to_string()
}

fn attrs(e: &quick_xml::events::BytesStart) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for a in e.attributes().flatten() {
        let key = local_name(a.key.as_ref());
        let val = a
            .normalized_value(XmlVersion::Implicit1_0)
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
    fn resolves_entity_references_in_text_and_attributes() {
        let xml = "<Doc><Nm> Smith &amp; Sons &#x41;G </Nm><Amt Ccy=\"&#69;UR\">1</Amt></Doc>";
        let root = parse(xml).unwrap();
        assert_eq!(root.text_at(&["Nm"]).as_deref(), Some("Smith & Sons AG"));
        assert_eq!(root.child("Amt").unwrap().attr("Ccy"), Some("EUR"));
    }

    #[test]
    fn malformed_is_none_or_partial() {
        assert!(parse("not xml at all").is_none() || parse("not xml").is_none());
        // A truncated tree returns what was parsed without panicking.
        let _ = parse("<Document><GrpHdr>");
    }
}
