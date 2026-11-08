#![no_main]
//! Fuzz target: CBOR decode must never panic, and anything it accepts must
//! re-encode and decode to the same value.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_tup::cbor;

fuzz_target!(|data: &[u8]| {
    if let Ok(dp) = cbor::decode_from_bytes(data) {
        // NaN != NaN, so only check round-trip equality for values that compare equal.
        if dp == dp {
            let re = cbor::encode_to_vec(&dp).expect("accepted message re-encodes");
            let back = cbor::decode_from_bytes(&re).expect("re-encoded message decodes");
            assert_eq!(dp, back);
        }
    }
});
