//! Parse an OCPP-J 1.6 BootNotification call (`[2, id, action, payload]`) and
//! normalize it into a TUP `DataPoint`.
//!
//! Run with: `cargo run -p tpt-protocol-ocpp --example parse_ocpp`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_ocpp::{OcppNormalizer, OcppParser};

fn main() {
    let text = r#"[2,"19223201","BootNotification",{"chargePointVendor":"VendorX","chargePointModel":"M1"}]"#;
    println!("frame: {text}");

    let frame = OcppParser
        .parse(text.as_bytes())
        .expect("a well-formed OCPP-J call parses");
    println!("parsed: {frame:?}");

    match OcppNormalizer.normalize(&frame) {
        Ok(dp) => println!(
            "normalized: {} {} = {:?} ({})",
            dp.source_protocol, dp.point_id, dp.value, dp.quality
        ),
        Err(e) => println!("no data point: {e:?}"),
    }
}
