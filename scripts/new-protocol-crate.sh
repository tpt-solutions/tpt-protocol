#!/usr/bin/env bash
#
# new-protocol-crate.sh — scaffold a new TPT protocol crate.
#
# Usage:  scripts/new-protocol-crate.sh <protocol-name>
#   e.g.  scripts/new-protocol-crate.sh zigbee
#
# Creates crates/tpt-protocol-<name>/ with a no_std lib skeleton, README, and
# registers it as a workspace member in the root Cargo.toml. After running,
# wire it into the daemon (crates/tpt-protocol-daemon/src/plugins/mod.rs) and
# add it to the workspace test/CI as needed.
#
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 <protocol-name>" >&2
  exit 1
fi

NAME="$1"
CRATE="tpt-protocol-${NAME}"
CRATE_DIR="crates/${CRATE}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -d "${ROOT}/${CRATE_DIR}" ]; then
  echo "error: ${CRATE_DIR} already exists" >&2
  exit 1
fi

echo "Creating ${CRATE_DIR} ..."
mkdir -p "${ROOT}/${CRATE_DIR}/src"

# --- Cargo.toml ------------------------------------------------------------
cat > "${ROOT}/${CRATE_DIR}/Cargo.toml" <<EOF
[package]
name = "${CRATE}"
version = "0.1.0"
description = "${NAME} protocol implementation for the TPT Protocol suite"
keywords = ["tpt", "protocol", "${NAME}"]
categories = ["network-programming", "embedded"]
authors.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
homepage.workspace = true
repository.workspace = true

[features]
default = ["std"]
std = ["tpt-protocol-tup/std"]
alloc = ["tpt-protocol-tup/alloc"]

[dependencies]
tpt-protocol-core = { workspace = true }
tpt-protocol-tup = { workspace = true }
EOF

# --- src/lib.rs ------------------------------------------------------------
cat > "${ROOT}/${CRATE_DIR}/src/lib.rs" <<EOF
//! ${NAME} protocol implementation for the TPT Protocol suite.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// A decoded ${NAME} frame.
pub struct ${NAME^}Frame {
    pub raw: Vec<u8>,
}

/// Parses ${NAME} wire frames.
pub struct ${NAME^}Parser;

impl FrameParser for ${NAME^}Parser {
    type Frame = ${NAME^}Frame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.is_empty() {
            return Err(Error::Parse("${NAME} frame too short".into()));
        }
        Ok(${NAME^}Frame { raw: bytes.to_vec() })
    }
}

/// Validates ${NAME} connection state.
pub struct ${NAME^}Validator;

impl StateValidator for ${NAME^}Validator {
    type State = ();

    fn validate(&self, _state: &Self::State) -> Result<(), Error> {
        Ok(())
    }
}

/// Normalizes ${NAME} frames into TUP data points.
pub struct ${NAME^}Normalizer;

impl DataNormalizer for ${NAME^}Normalizer {
    type Frame = ${NAME^}Frame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        Ok(DataPoint::new(
            SourceProtocol::Modbus, // TODO: add a dedicated SourceProtocol variant
            "device".to_string(),
            "point".to_string(),
            0,
            TupValue::Bytes(frame.raw.clone()),
            Quality::Good,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_parse() {
        let parser = ${NAME^}Parser;
        let frame = parser.parse(&[0x01, 0x02, 0x03]).unwrap();
        assert_eq!(frame.raw.len(), 3);
    }
}
EOF

# --- README.md -------------------------------------------------------------
cat > "${ROOT}/${CRATE_DIR}/README.md" <<EOF
# ${CRATE}

${NAME} protocol implementation for the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) suite.

Normalizes ${NAME} frames into the TPT Unified Protocol (TUP). \`#![no_std]\`,
depends only on \`tpt-protocol-core\` / \`tpt-protocol-tup\`.
EOF

# --- Register as workspace member ------------------------------------------
# Insert the new member alphabetically-ish right after the tpt-protocol-core line.
python3 - "$ROOT" "$CRATE" <<'PY'
import sys, re
root, crate = sys.argv[1], sys.argv[2]
cargo_toml = f"{root}/Cargo.toml"
text = open(cargo_toml).read()
member = f'    "crates/{crate}",\n'
if f'crates/{crate}"' in text:
    print("already a member")
else:
    # Insert after the tpt-protocol-core member line if present, else append.
    pat = re.compile(r'("crates/tpt-protocol-core",\n)')
    if pat.search(text):
        text = pat.sub(r'\1' + member, text, count=1)
    else:
        text = text.replace("members = [", "members = [\n" + member)
    open(cargo_toml, "w").write(text)
    print("registered workspace member")
PY

echo "Done. Implement parse/validate/normalize in crates/${CRATE}/src/lib.rs"
echo "and wire it into the daemon plugins module."
