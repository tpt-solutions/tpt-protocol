//! Parse an IEEE 11073 SDC episodic metric report (a SOAP 1.2 envelope carrying
//! two numeric observations) and normalize each observation into a TUP point.
//!
//! The XML element and attribute names follow the SDC draft used by the crate;
//! see the crate README for what is still unverified against a real device.
//!
//! Run with: `cargo run -p tpt-protocol-ieee11073-sdc --example parse_sdc`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_ieee11073_sdc::{SdcNormalizer, SdcParser};

const ENVELOPE_OPEN: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<s12:Envelope xmlns:s12="http://www.w3.org/2003/05/soap-envelope"
              xmlns:wsa="http://www.w3.org/2005/08/addressing"
              xmlns:pm="http://standards.ieee.org/downloads/11073/11073-10207-2017/participant">
  <s12:Header>
    <wsa:Action>http://standards.ieee.org/downloads/11073/11073-20701-2018/ServiceModel/Waveform</wsa:Action>
    <wsa:MessageID>urn:uuid:1234</wsa:MessageID>
  </s12:Header>
  <s12:Body>"#;

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

fn main() {
    let message = format!("{ENVELOPE_OPEN}{TWO_NUMERICS}</s12:Body></s12:Envelope>");
    println!("frame: {} bytes of SOAP/XML", message.len());

    let frame = SdcParser
        .parse(message.as_bytes())
        .expect("a well-formed SDC report parses");
    println!("parsed: {frame:?}");

    match SdcNormalizer.normalize_all(&frame) {
        Ok(points) => {
            for dp in points {
                println!(
                    "normalized: {} {} = {:?} ({})",
                    dp.source_protocol, dp.point_id, dp.value, dp.quality
                );
            }
        }
        Err(e) => println!("no data points: {e:?}"),
    }
}
