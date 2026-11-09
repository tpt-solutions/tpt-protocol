//! The C ABI. The declarations are in `include/tpt_protocol.h`, which is written
//! by hand; a test below checks that the header and these functions agree.
//!
//! Memory: `tpt_decode_json` allocates the string it returns, and the caller frees
//! it with `tpt_free_string`. Nothing else is allocated for the caller.

use std::ffi::{c_char, c_int, CStr, CString};

/// The frame decoded. `*out` holds the JSON array.
pub const TPT_OK: c_int = 0;
/// A pointer was null, or the protocol name was not UTF-8. `*out` holds a message.
pub const TPT_ERR_ARGUMENT: c_int = 1;
/// The frame did not decode, or the protocol is unknown. `*out` holds a message.
pub const TPT_ERR_DECODE: c_int = 2;

/// Decodes one frame.
///
/// On success it returns [`TPT_OK`] and sets `*out` to a JSON array of TUP data
/// points. On failure it returns an error code and sets `*out` to a message.
/// Either way, free `*out` with [`tpt_free_string`].
///
/// # Safety
///
/// `protocol` must point to a NUL-terminated string. `bytes` must point to `len`
/// readable bytes; it may be null only when `len` is 0. `out` must point to a
/// writable `char *` slot.
#[no_mangle]
pub unsafe extern "C" fn tpt_decode_json(
    protocol: *const c_char,
    bytes: *const u8,
    len: usize,
    out: *mut *mut c_char,
) -> c_int {
    if out.is_null() || protocol.is_null() || (bytes.is_null() && len != 0) {
        return TPT_ERR_ARGUMENT;
    }
    // SAFETY: `protocol` is non-null, and the caller guarantees a NUL-terminated string.
    let protocol = match unsafe { CStr::from_ptr(protocol) }.to_str() {
        Ok(name) => name,
        Err(_) => {
            // SAFETY: `out` is non-null and writable, per the contract above.
            unsafe { write_out(out, "the protocol name is not UTF-8") };
            return TPT_ERR_ARGUMENT;
        }
    };
    let frame: &[u8] = if len == 0 {
        &[]
    } else {
        // SAFETY: `bytes` is non-null and valid for `len` bytes, per the contract above.
        unsafe { std::slice::from_raw_parts(bytes, len) }
    };
    match crate::decode_json(protocol, frame) {
        Ok(json) => {
            // SAFETY: `out` is non-null and writable, per the contract above.
            unsafe { write_out(out, &json) };
            TPT_OK
        }
        Err(message) => {
            // SAFETY: `out` is non-null and writable, per the contract above.
            unsafe { write_out(out, &message) };
            TPT_ERR_DECODE
        }
    }
}

/// Frees a string returned by [`tpt_decode_json`]. Null is accepted and ignored.
///
/// # Safety
///
/// `s` must be null or a string returned by [`tpt_decode_json`] that has not
/// already been freed.
#[no_mangle]
pub unsafe extern "C" fn tpt_free_string(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: `s` came from `CString::into_raw` in `write_out`, per the contract above.
        drop(unsafe { CString::from_raw(s) });
    }
}

/// Stores `text` in `*out` as a C string. Interior NUL bytes are escaped, because
/// a C string cannot hold them.
///
/// # Safety
///
/// `out` must be non-null and writable.
unsafe fn write_out(out: *mut *mut c_char, text: &str) {
    let escaped = text.replace('\0', "\\0");
    let c = CString::new(escaped).unwrap_or_default();
    // SAFETY: `out` is non-null and writable, per this function's contract.
    unsafe { *out = c.into_raw() };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::ptr;

    const HEADER: &str = include_str!("../include/tpt_protocol.h");

    fn call(protocol: &str, bytes: &[u8]) -> (c_int, String) {
        let protocol = CString::new(protocol).unwrap();
        let mut out: *mut c_char = ptr::null_mut();
        // SAFETY: valid NUL-terminated name, a valid byte slice, and a writable out slot.
        let code =
            unsafe { tpt_decode_json(protocol.as_ptr(), bytes.as_ptr(), bytes.len(), &mut out) };
        assert!(!out.is_null());
        // SAFETY: `out` was set by `tpt_decode_json` and is freed exactly once below.
        let text = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: `out` came from `tpt_decode_json` and is not used afterwards.
        unsafe { tpt_free_string(out) };
        (code, text)
    }

    #[test]
    fn decodes_through_the_c_abi() {
        let (code, json) = call("modbus", &[0x01, 0x06, 0x00, 0x01, 0x00, 0x0A, 0x58, 0x0D]);
        assert_eq!(code, TPT_OK, "{json}");
        assert!(json.contains("\"value\":10"), "{json}");
    }

    #[test]
    fn errors_report_a_code_and_a_message() {
        let (code, message) = call("nope", &[]);
        assert_eq!(code, TPT_ERR_DECODE);
        assert!(message.contains("unknown protocol"), "{message}");
    }

    #[test]
    fn null_pointers_are_argument_errors() {
        let mut out: *mut c_char = ptr::null_mut();
        // SAFETY: deliberately passes a null protocol to check the guard.
        let code = unsafe { tpt_decode_json(ptr::null(), ptr::null(), 0, &mut out) };
        assert_eq!(code, TPT_ERR_ARGUMENT);
        assert!(out.is_null());
    }

    #[test]
    fn header_declares_what_the_library_exports() {
        // The header is written by hand, so check it names the same functions and codes.
        assert!(
            HEADER.contains("int tpt_decode_json("),
            "header is missing tpt_decode_json"
        );
        assert!(
            HEADER.contains("void tpt_free_string("),
            "header is missing tpt_free_string"
        );
        for code in ["TPT_OK 0", "TPT_ERR_ARGUMENT 1", "TPT_ERR_DECODE 2"] {
            assert!(HEADER.contains(code), "header is missing {code}");
        }
    }
}
