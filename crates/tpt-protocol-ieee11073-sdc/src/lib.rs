//! IEEE 11073 SDC (Service-Oriented Device Connectivity) for medical devices.
//!
//! SDC messages are SOAP 1.2 envelopes whose bodies carry BICEPS XML
//! (ISO/IEEE 11073-10207 and 11073-20701). This crate has a bounded XML
//! parser, reads the SOAP envelope and its WS-Addressing headers, and extracts
//! numeric metric observations.
//!
//! Element matching is by local name. The BICEPS namespace version could not
//! be checked offline, so the element and attribute names used here
//! (`NumericMetricState`, `MetricValue`, `DescriptorHandle`, `Value`) are the
//! ones documented in the standard as this crate was written. Verify them
//! against a real device before relying on them.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// SOAP 1.2 envelope namespace.
const NS_SOAP: &str = "http://www.w3.org/2003/05/soap-envelope";
/// WS-Addressing namespace (Action and MessageID headers).
const NS_WSA: &str = "http://www.w3.org/2005/08/addressing";

/// Deepest element nesting accepted.
const MAX_DEPTH: usize = 32;
/// Most elements accepted in one message.
const MAX_ELEMENTS: usize = 4096;

// ---------------------------------------------------------------------------
// XML
// ---------------------------------------------------------------------------

/// An XML element with its resolved namespace and its text and children.
#[derive(Debug, Clone, PartialEq)]
struct Element {
    namespace: String,
    local: String,
    attrs: Vec<(String, String)>,
    children: Vec<Element>,
    /// The character data directly inside the element, with entities decoded.
    text: String,
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn child(&self, namespace: &str, local: &str) -> Option<&Element> {
        self.children
            .iter()
            .find(|c| c.namespace == namespace && c.local == local)
    }

    /// Descendants with a given local name, in document order.
    fn descendants<'a>(&'a self, local: &str, out: &mut Vec<&'a Element>) {
        for child in &self.children {
            if child.local == local {
                out.push(child);
            }
            child.descendants(local, out);
        }
    }
}

struct Cursor<'a> {
    src: &'a str,
    pos: usize,
    elements: usize,
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':')
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

impl<'a> Cursor<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, token: &str) -> Result<(), Error> {
        if self.rest().starts_with(token) {
            self.pos += token.len();
            Ok(())
        } else {
            Err(Error::Parse(format!(
                "XML expected '{token}' at byte {}",
                self.pos
            )))
        }
    }

    /// Reads a name (an element or attribute name, possibly with a prefix).
    fn name(&mut self) -> Result<&'a str, Error> {
        let start = self.pos;
        while self.peek().is_some_and(is_name_byte) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(Error::Parse(format!("XML expected a name at byte {start}")));
        }
        Ok(&self.src[start..self.pos])
    }

    /// Skips to just after `end`, returning the text before it.
    fn until(&mut self, end: &str) -> Result<&'a str, Error> {
        let rest = self.rest();
        let idx = rest
            .find(end)
            .ok_or_else(|| Error::Parse(format!("XML missing '{end}'")))?;
        let text = &rest[..idx];
        self.pos += idx + end.len();
        Ok(text)
    }
}

/// Decodes the predefined entities and numeric character references.
fn decode_entities(raw: &str) -> Result<String, Error> {
    if !raw.contains('&') {
        return Ok(raw.to_string());
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let semi = after
            .find(';')
            .filter(|&i| i <= 10)
            .ok_or_else(|| Error::Parse("XML entity reference is not terminated".into()))?;
        let entity = &after[..semi];
        match entity {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            other => {
                let code = if let Some(hex) = other.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = other.strip_prefix('#') {
                    dec.parse::<u32>().ok()
                } else {
                    None
                };
                let ch = code
                    .and_then(char::from_u32)
                    .ok_or_else(|| Error::Parse(format!("XML unknown entity &{other};")))?;
                out.push(ch);
            }
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn resolve(scope: &[(String, String)], prefix: &str) -> String {
    scope
        .iter()
        .rev()
        .find(|(p, _)| p == prefix)
        .map(|(_, uri)| uri.clone())
        .unwrap_or_default()
}

/// Parses one element. The cursor is at its `<`.
fn element(
    cur: &mut Cursor<'_>,
    scope: &mut Vec<(String, String)>,
    depth: usize,
) -> Result<Element, Error> {
    if depth >= MAX_DEPTH {
        return Err(Error::Parse("XML nesting is too deep".into()));
    }
    cur.elements += 1;
    if cur.elements > MAX_ELEMENTS {
        return Err(Error::Parse("XML has too many elements".into()));
    }
    cur.expect("<")?;
    let qname = cur.name()?;

    let scope_mark = scope.len();
    let mut raw_attrs: Vec<(&str, String)> = Vec::new();
    loop {
        cur.skip_space();
        match cur.peek() {
            Some(b'/') | Some(b'>') => break,
            Some(_) => {
                let attr = cur.name()?;
                cur.skip_space();
                cur.expect("=")?;
                cur.skip_space();
                let quote = match cur.peek() {
                    Some(q @ (b'"' | b'\'')) => q,
                    _ => return Err(Error::Parse("XML attribute value must be quoted".into())),
                };
                cur.pos += 1;
                let value = cur.until(if quote == b'"' { "\"" } else { "'" })?;
                let value = decode_entities(value)?;
                if attr == "xmlns" {
                    scope.push((String::new(), value.clone()));
                } else if let Some(prefix) = attr.strip_prefix("xmlns:") {
                    scope.push((prefix.to_string(), value.clone()));
                }
                if raw_attrs.iter().any(|(n, _)| *n == attr) {
                    return Err(Error::Parse(format!("XML duplicate attribute {attr}")));
                }
                raw_attrs.push((attr, value));
            }
            None => return Err(Error::Parse("XML tag is not closed".into())),
        }
    }

    let (prefix, local) = match qname.split_once(':') {
        Some((p, l)) => (p, l),
        None => ("", qname),
    };
    let namespace = resolve(scope, prefix);
    let attrs: Vec<(String, String)> = raw_attrs
        .into_iter()
        .map(|(n, v)| (n.to_string(), v))
        .collect();
    let mut element = Element {
        namespace,
        local: local.to_string(),
        attrs,
        children: Vec::new(),
        text: String::new(),
    };

    if cur.rest().starts_with("/>") {
        cur.pos += 2;
        scope.truncate(scope_mark);
        return Ok(element);
    }
    cur.expect(">")?;

    loop {
        if cur.rest().starts_with("</") {
            cur.pos += 2;
            let end = cur.name()?;
            if end != qname {
                return Err(Error::Parse(format!("XML </{end}> closes <{qname}>")));
            }
            cur.skip_space();
            cur.expect(">")?;
            break;
        } else if cur.rest().starts_with("<!--") {
            cur.pos += 4;
            cur.until("-->")?;
        } else if cur.rest().starts_with("<![CDATA[") {
            cur.pos += 9;
            element.text.push_str(cur.until("]]>")?);
        } else if cur.rest().starts_with("<?") {
            cur.pos += 2;
            cur.until("?>")?;
        } else if cur.rest().starts_with("<!") {
            return Err(Error::Parse(
                "XML declarations such as DOCTYPE are not accepted".into(),
            ));
        } else if cur.peek() == Some(b'<') {
            let child = element_child(cur, scope, depth + 1)?;
            element.children.push(child);
        } else if cur.peek().is_none() {
            return Err(Error::Parse(format!("XML <{qname}> is not closed")));
        } else {
            let end = cur.rest().find('<').unwrap_or(cur.rest().len());
            let text = &cur.rest()[..end];
            element.text.push_str(&decode_entities(text)?);
            cur.pos += end;
        }
    }
    scope.truncate(scope_mark);
    Ok(element)
}

fn element_child(
    cur: &mut Cursor<'_>,
    scope: &mut Vec<(String, String)>,
    depth: usize,
) -> Result<Element, Error> {
    element(cur, scope, depth)
}

/// Parses a document with one root element. Prolog, comments and processing
/// instructions outside the root are skipped.
fn parse_document(src: &str) -> Result<Element, Error> {
    let mut cur = Cursor {
        src,
        pos: 0,
        elements: 0,
    };
    let mut scope = Vec::new();
    loop {
        cur.skip_space();
        if cur.rest().starts_with("<?") {
            cur.pos += 2;
            cur.until("?>")?;
        } else if cur.rest().starts_with("<!--") {
            cur.pos += 4;
            cur.until("-->")?;
        } else if cur.rest().starts_with("<!") {
            return Err(Error::Parse(
                "XML declarations such as DOCTYPE are not accepted".into(),
            ));
        } else {
            break;
        }
    }
    let root = element(&mut cur, &mut scope, 0)?;
    cur.skip_space();
    if cur.pos != src.len() {
        return Err(Error::Parse(
            "XML has content after the root element".into(),
        ));
    }
    Ok(root)
}

// ---------------------------------------------------------------------------
// SOAP and BICEPS
// ---------------------------------------------------------------------------

/// A numeric metric observation from a state element.
#[derive(Debug, Clone, PartialEq)]
pub struct NumericObservation {
    pub descriptor_handle: String,
    pub value: f64,
}

/// A decoded SDC message: the SOAP operation and its numeric observations.
#[derive(Debug, Clone, PartialEq)]
pub struct SdcFrame {
    /// `wsa:Action` from the SOAP header, when present.
    pub action: Option<String>,
    /// `wsa:MessageID` from the SOAP header, when present.
    pub message_id: Option<String>,
    /// Local name of the first element in the SOAP body (the operation).
    pub operation: String,
    pub observations: Vec<NumericObservation>,
}

/// Reads the numeric observations of a body: each `NumericMetricState` element
/// with a `MetricValue` child carrying a finite `Value`.
fn numeric_observations(body: &Element) -> Vec<NumericObservation> {
    let mut states = Vec::new();
    body.descendants("NumericMetricState", &mut states);
    states
        .into_iter()
        .filter_map(|state| {
            let descriptor_handle = state.attr("DescriptorHandle")?.to_string();
            let value: f64 = state
                .children
                .iter()
                .find(|c| c.local == "MetricValue")?
                .attr("Value")?
                .parse()
                .ok()?;
            value.is_finite().then_some(NumericObservation {
                descriptor_handle,
                value,
            })
        })
        .collect()
}

/// Parses SDC SOAP messages.
pub struct SdcParser;

impl FrameParser for SdcParser {
    type Frame = SdcFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        let text = core::str::from_utf8(bytes)
            .map_err(|_| Error::Parse("SDC message is not UTF-8".into()))?;
        let envelope = parse_document(text)?;
        if envelope.local != "Envelope" || envelope.namespace != NS_SOAP {
            return Err(Error::Parse(
                "SDC message is not a SOAP 1.2 envelope".into(),
            ));
        }

        let header = envelope.child(NS_SOAP, "Header");
        let action = header
            .and_then(|h| h.child(NS_WSA, "Action"))
            .map(|a| a.text.trim().to_string());
        let message_id = header
            .and_then(|h| h.child(NS_WSA, "MessageID"))
            .map(|m| m.text.trim().to_string());

        let body = envelope
            .child(NS_SOAP, "Body")
            .ok_or_else(|| Error::Parse("SOAP envelope has no Body".into()))?;
        let operation = body
            .children
            .first()
            .ok_or_else(|| Error::Parse("SOAP Body is empty".into()))?
            .local
            .clone();

        Ok(SdcFrame {
            action,
            message_id,
            operation,
            observations: numeric_observations(body),
        })
    }
}

/// Connection state for an SDC device.
#[derive(Debug, Clone, PartialEq)]
pub struct SdcConnectionState {
    pub connected: bool,
    pub metric_count: usize,
    pub last_sequence_id: u64,
}

pub struct SdcValidator;

impl StateValidator for SdcValidator {
    type State = SdcConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("sdc: not connected".into()));
        }
        if state.metric_count > 1000 {
            return Err(Error::State("sdc: too many metrics".into()));
        }
        Ok(())
    }
}

/// Normalizes SDC numeric observations into TUP DataPoints. The device is the
/// SDC source (no device identity is carried in a notification), and the point
/// is the metric's descriptor handle.
pub struct SdcNormalizer;

impl SdcNormalizer {
    /// One data point per numeric observation.
    pub fn points(&self, frame: &SdcFrame) -> Vec<DataPoint> {
        frame
            .observations
            .iter()
            .map(|o| {
                DataPoint::new(
                    SourceProtocol::Ieee11073Sdc,
                    "sdc:device".to_string(),
                    o.descriptor_handle.clone(),
                    0,
                    TupValue::Float(o.value),
                    Quality::Good,
                )
            })
            .collect()
    }
}

impl DataNormalizer for SdcNormalizer {
    type Frame = SdcFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        self.points(frame)
            .into_iter()
            .next()
            .ok_or_else(|| Error::Normalize("SDC message has no numeric observation".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    const ENVELOPE_OPEN: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<s12:Envelope xmlns:s12="http://www.w3.org/2003/05/soap-envelope"
              xmlns:wsa="http://www.w3.org/2005/08/addressing"
              xmlns:pm="http://standards.ieee.org/downloads/11073/11073-10207-2017/participant">
  <s12:Header>
    <wsa:Action>http://standards.ieee.org/downloads/11073/11073-20701-2018/ServiceModel/Waveform</wsa:Action>
    <wsa:MessageID>urn:uuid:1234</wsa:MessageID>
  </s12:Header>
  <s12:Body>"#;

    fn message(body: &str) -> String {
        format!("{ENVELOPE_OPEN}{body}</s12:Body></s12:Envelope>")
    }

    const TWO_NUMERICS: &str = r#"
    <pm:EpisodicMetricReport>
      <pm:ReportPart>
        <pm:NumericMetricState DescriptorHandle="mds0.vmd0.metric1" StateVersion="1">
          <pm:MetricValue Value="37.5"/>
        </pm:NumericMetricState>
        <pm:NumericMetricState DescriptorHandle="mds0.vmd0.metric2">
          <pm:MetricValue Value="98.2"/>
        </pm:NumericMetricState>
      </pm:ReportPart>
    </pm:EpisodicMetricReport>"#;

    #[test]
    fn soap_header_and_operation_are_read() {
        let frame = SdcParser.parse(message(TWO_NUMERICS).as_bytes()).unwrap();
        assert_eq!(frame.operation, "EpisodicMetricReport");
        assert_eq!(frame.message_id.as_deref(), Some("urn:uuid:1234"));
        assert!(frame.action.as_deref().unwrap().ends_with("/Waveform"));
    }

    #[test]
    fn numeric_observations_are_extracted_in_order() {
        let frame = SdcParser.parse(message(TWO_NUMERICS).as_bytes()).unwrap();
        assert_eq!(
            frame.observations,
            vec![
                NumericObservation {
                    descriptor_handle: "mds0.vmd0.metric1".into(),
                    value: 37.5
                },
                NumericObservation {
                    descriptor_handle: "mds0.vmd0.metric2".into(),
                    value: 98.2
                },
            ]
        );
    }

    #[test]
    fn normalizer_emits_one_point_per_observation() {
        let frame = SdcParser.parse(message(TWO_NUMERICS).as_bytes()).unwrap();
        let points = SdcNormalizer.points(&frame);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].point_id, "mds0.vmd0.metric1");
        assert_eq!(points[0].value, TupValue::Float(37.5));
        assert_eq!(points[0].source_protocol, SourceProtocol::Ieee11073Sdc);
        assert_eq!(
            SdcNormalizer.normalize(&frame).unwrap().point_id,
            "mds0.vmd0.metric1"
        );
    }

    #[test]
    fn state_without_a_value_is_skipped() {
        let body = r#"<pm:R><pm:NumericMetricState DescriptorHandle="m"/></pm:R>"#;
        let frame = SdcParser.parse(message(body).as_bytes()).unwrap();
        assert!(frame.observations.is_empty());
        assert!(SdcNormalizer.normalize(&frame).is_err());
    }

    #[test]
    fn non_finite_value_is_skipped() {
        let body = r#"<pm:R><pm:NumericMetricState DescriptorHandle="m"><pm:MetricValue Value="NaN"/></pm:NumericMetricState></pm:R>"#;
        let frame = SdcParser.parse(message(body).as_bytes()).unwrap();
        assert!(frame.observations.is_empty());
    }

    #[test]
    fn entities_in_attributes_and_text_are_decoded() {
        let body = r#"<pm:R><pm:NumericMetricState DescriptorHandle="a&amp;b&#x41;"><pm:MetricValue Value="1"/></pm:NumericMetricState></pm:R>"#;
        let frame = SdcParser.parse(message(body).as_bytes()).unwrap();
        assert_eq!(frame.observations[0].descriptor_handle, "a&bA");
    }

    #[test]
    fn non_soap_root_is_rejected() {
        let xml = r#"<root xmlns="urn:example"><Body/></root>"#;
        assert!(SdcParser.parse(xml.as_bytes()).is_err());
    }

    #[test]
    fn wrong_soap_namespace_is_rejected() {
        let xml = r#"<Envelope xmlns="http://schemas.xmlsoap.org/soap/envelope/"><Body><X/></Body></Envelope>"#;
        assert!(SdcParser.parse(xml.as_bytes()).is_err());
    }

    #[test]
    fn doctype_is_rejected() {
        let xml = format!(
            "<!DOCTYPE x [<!ENTITY e \"boom\">]>{}",
            message(TWO_NUMERICS)
        );
        assert!(SdcParser.parse(xml.as_bytes()).is_err());
    }

    #[test]
    fn mismatched_tags_are_rejected() {
        let xml = format!("{ENVELOPE_OPEN}<pm:A></pm:B></s12:Body></s12:Envelope>");
        assert!(SdcParser.parse(xml.as_bytes()).is_err());
    }

    #[test]
    fn unterminated_document_is_rejected() {
        let xml = format!("{ENVELOPE_OPEN}<pm:A>");
        assert!(SdcParser.parse(xml.as_bytes()).is_err());
    }

    #[test]
    fn unknown_entity_is_rejected() {
        let xml = message("<pm:A>&nbsp;</pm:A>");
        assert!(SdcParser.parse(xml.as_bytes()).is_err());
    }

    #[test]
    fn deep_nesting_is_bounded() {
        let mut body = String::new();
        for _ in 0..40 {
            body.push_str("<pm:D>");
        }
        for _ in 0..40 {
            body.push_str("</pm:D>");
        }
        assert!(SdcParser.parse(message(&body).as_bytes()).is_err());
    }

    #[test]
    fn non_utf8_is_rejected() {
        assert!(SdcParser.parse(&[0xFF, 0xFE, 0x00]).is_err());
    }

    #[test]
    fn validator_rules() {
        let ok = SdcConnectionState {
            connected: true,
            metric_count: 3,
            last_sequence_id: 1,
        };
        assert!(SdcValidator.validate(&ok).is_ok());
        let down = SdcConnectionState {
            connected: false,
            ..ok.clone()
        };
        assert!(SdcValidator.validate(&down).is_err());
        let many = SdcConnectionState {
            metric_count: 5000,
            ..ok
        };
        assert!(SdcValidator.validate(&many).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = SdcParser.parse(&bytes);
        }
    }
}
