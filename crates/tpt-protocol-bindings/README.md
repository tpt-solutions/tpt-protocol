# tpt-protocol-bindings

Language bindings for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol)
suite: decode one industrial, medical or hardware frame to TUP JSON from C, from
WebAssembly, or from Python.

This crate is not published (`publish = false`). It is built from the repository for use
by other languages.

## Overview

All three front ends share one dispatch table. Each protocol is a parser and a normalizer.
A frame goes through `parse`, then `normalize_all`, and every resulting data point is
written as the same JSON object the daemon sends over its WebSocket. So a frame decoded
here looks the same as one received from the daemon.

| Front end | Build | Entry points |
| --- | --- | --- |
| C | default (`cdylib`, `rlib`) | `tpt_decode_json`, `tpt_free_string`, in `include/tpt_protocol.h` |
| WebAssembly | `--features wasm` | `decode_hex(protocol, hex)`, `protocols()` |
| Python | `--features python` (maturin) | `decode(protocol, data)`, `protocols()` |

## Protocols

The `protocols()` list, and every front end, accepts these 19 names:

`modbus`, `j1939`, `isobus`, `uds`, `canopen`, `doip-someip`, `nmea`, `mavlink`, `bacnet`,
`opcua`, `dnp3`, `ethercat`, `sparkplugb`, `ocpp`, `ieee11073-sdc`, `hl7v2`, `knx`,
`matter-thread`, `coap`.

An unknown name is an error, not a silent empty result.

## Output

The decoded result is a JSON array of data points, for example:

```json
[{"schema_version":1,"source_protocol":"modbus","device_id":"modbus:1",
  "point_id":"holding_register[0]","timestamp":0,"value":{"int":10},"quality":"good"}]
```

The field names and value tags come from `tpt-protocol-tup`, and are checked against
`schema/tup-v1.schema.json`. A frame with no measured value (for example a request) is an
error rather than an empty array.

## C

The C ABI is in [`src/ffi.rs`](src/ffi.rs), and the declarations are in
[`include/tpt_protocol.h`](include/tpt_protocol.h). The header is written by hand. A unit
test checks that it declares what the library exports.

```c
#include "tpt_protocol.h"

char *json = NULL;
int rc = tpt_decode_json("modbus", bytes, len, &json);
if (rc == TPT_OK) {
    /* json is a JSON array of data points */
} else {
    /* json holds the error message */
}
tpt_free_string(json);
```

- `tpt_decode_json(protocol, bytes, len, &out)` takes a NUL-terminated protocol name and
  `len` bytes. It returns `TPT_OK` (0), `TPT_ERR_ARGUMENT` (1) for a null pointer or a
  non-UTF-8 name, or `TPT_ERR_DECODE` (2) for a frame that does not decode. On both success
  and error, `*out` is set to a string the caller owns.
- `tpt_free_string(s)` frees a string returned through `out`. It accepts `NULL`. Every
  returned string must be freed, on success and on error.

## WebAssembly

```bash
cargo build -p tpt-protocol-bindings --features wasm --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir www/pkg \
  ../../target/wasm32-unknown-unknown/debug/tpt_protocol.wasm
```

`wasm-bindgen` is pinned to `=0.2.126` in `Cargo.toml`, because the CLI and the library
must be the same version. `decode_hex` accepts hex text, with or without a `0x` prefix, and
returns the JSON array. The browser playground in [`www/`](www/) runs entirely in the
page. Serve the folder over HTTP (for example `python -m http.server`), because a `file://`
page cannot load WebAssembly modules.

## Python

```bash
pip install maturin
maturin develop --release          # into the current virtualenv
```

```python
import tpt_protocol

tpt_protocol.protocols()                        # the 19 names above
tpt_protocol.decode("modbus", frame_bytes)      # a JSON string: an array of data points
tpt_protocol.decode("nope", b"")                # raises ValueError
```

`pyproject.toml` builds the wheel with maturin, using the `python` feature and the module
name `tpt_protocol`. It requires Python 3.9 or later. The CI job `python-wheel` builds the
wheel on every push, and the wheel was installed in a clean virtualenv and checked for
`protocols()`, `decode()` and the `ValueError` on an unknown name.

## Crate layout

- `src/lib.rs` — the dispatch table (`protocols!`), `decode_json`, `decode_hex`, and the
  unit tests.
- `src/ffi.rs` — the C ABI and its tests.
- `src/wasm.rs` — the WebAssembly exports (`--features wasm`).
- `src/python.rs` — the Python module (`--features python`).
- `include/tpt_protocol.h` — the C header.
- `www/` — the browser playground.
- `pyproject.toml` — the maturin build for the Python module.

## Status / Conformance

- **Decode only.** The bindings parse one frame and return its data points. They do not
  hold a connection, stream, or validate connection state.
- **C header is hand-written.** It is checked by a unit test, not generated. Change the
  functions and the header together.
- **Python and WebAssembly are built, not published.** The wheel is not uploaded to PyPI,
  and the WebAssembly package is generated into `www/pkg` on demand.
- **Same coverage as the crates.** A protocol's decode is only as complete as its crate; see
  each protocol's README.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` and `DataNormalizer` traits
  the dispatch uses.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the data point and its JSON form.
- [`tpt-protocol-daemon`](../tpt-protocol-daemon) — streams the same JSON over WebSocket.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
