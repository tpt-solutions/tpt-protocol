//! Workspace tasks for TPT Protocol. Run them with `cargo xtask <task>`.
//!
//! ```text
//! cargo xtask new-protocol <name>             scaffold and wire a protocol
//! cargo xtask --dry-run new-protocol <name>   show what would change, write nothing
//! cargo xtask --root <dir> new-protocol <name>  operate on another checkout (for testing)
//! ```
//!
//! `new-protocol` creates `crates/tpt-protocol-<name>` (parser, validator,
//! normalizer, tests, a proptest "never panics" check, an example and a fuzz
//! target), adds the crate to the workspace, adds a `SourceProtocol` variant
//! and its name to the wire schema (`schema/tup-v1.cddl`,
//! `schema/tup-v1.schema.json`), and wires the crate into the daemon.
//!
//! Every edit is computed in memory first. If any anchor in an existing file is
//! missing, nothing is written, so a half-wired protocol is never left behind.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "usage: cargo xtask [--dry-run] [--root <dir>] new-protocol <name>";

fn run(args: Vec<String>) -> Result<(), String> {
    let mut root = None;
    let mut dry_run = false;
    let mut positional = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--root" => {
                root = Some(PathBuf::from(
                    iter.next().ok_or("--root needs a directory")?,
                ))
            }
            "--dry-run" => dry_run = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => positional.push(arg),
        }
    }
    let root = match root {
        Some(root) => root,
        None => workspace_root()?,
    };
    match positional.as_slice() {
        [task, name] if task == "new-protocol" => new_protocol(&root, name, dry_run),
        _ => Err(USAGE.to_string()),
    }
}

/// `xtask` lives at `<root>/xtask`, so the root is its manifest directory's parent.
fn workspace_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "cannot find the workspace root".to_string())
}

/// The spellings a protocol name needs in each place it appears.
#[derive(Debug, PartialEq)]
struct Names {
    /// `modbus`, `matter-thread`: the crate suffix and the protocol name on the wire.
    name: String,
    /// `tpt-protocol-matter-thread`.
    crate_name: String,
    /// `matter_thread`: the Rust identifier of the crate, and the example's name.
    ident: String,
    /// `MatterThread`: the `SourceProtocol` variant and the type-name prefix.
    variant: String,
}

fn names(name: &str) -> Result<Names, String> {
    let mut chars = name.chars();
    let starts_well = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let body_ok = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !starts_well || !body_ok || name.ends_with('-') || name.contains("--") {
        return Err(format!(
            "'{name}' is not a valid protocol name: use lowercase letters, digits and single \
             hyphens, starting with a letter (for example 'zigbee' or 'matter-thread')"
        ));
    }
    let variant: String = name
        .split('-')
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect();
    Ok(Names {
        name: name.to_string(),
        crate_name: format!("tpt-protocol-{name}"),
        ident: name.replace('-', "_"),
        variant,
    })
}

/// A file to write, with its complete new contents.
struct Write {
    path: PathBuf,
    contents: String,
    why: &'static str,
}

fn new_protocol(root: &Path, name: &str, dry_run: bool) -> Result<(), String> {
    let n = names(name)?;
    let crate_dir = root.join("crates").join(&n.crate_name);
    if crate_dir.exists() {
        return Err(format!("{} already exists", crate_dir.display()));
    }

    let mut writes = Vec::new();
    // New files.
    writes.push(new_file(
        crate_dir.join("Cargo.toml"),
        cargo_toml(&n),
        "crate manifest",
    ));
    writes.push(new_file(
        crate_dir.join("src/lib.rs"),
        lib_rs(&n),
        "crate source",
    ));
    writes.push(new_file(
        crate_dir.join("README.md"),
        readme(&n),
        "crate README",
    ));
    writes.push(new_file(
        crate_dir
            .join("examples")
            .join(format!("parse_{}.rs", n.ident)),
        example(&n),
        "example",
    ));
    writes.push(new_file(
        root.join("fuzz/fuzz_targets")
            .join(format!("{}.rs", n.ident)),
        fuzz_target(&n),
        "fuzz target",
    ));

    // Edits to existing files. Each is computed from the current text; a missing anchor aborts.
    writes.push(edit_file(
        root.join("Cargo.toml"),
        |text| {
            insert_before(
                text,
                "    \"crates/tpt-protocol-daemon\",\n",
                &format!("    \"crates/{}\",\n", n.crate_name),
            )
        },
        "workspace member",
    )?);
    writes.push(edit_file(
        root.join("crates/tpt-protocol-tup/src/source_protocol.rs"),
        |text| add_source_protocol_variant(text, &n),
        "SourceProtocol variant",
    )?);
    writes.push(edit_file(
        root.join("crates/tpt-protocol-tup/src/cddl_conformance.rs"),
        |text| add_conformance_entry(text, &n),
        "conformance list",
    )?);
    writes.push(edit_file(
        root.join("schema/tup-v1.cddl"),
        |text| add_cddl_protocol(text, &n),
        "CDDL protocol list",
    )?);
    writes.push(edit_file(
        root.join("schema/tup-v1.schema.json"),
        |text| add_json_schema_protocol(text, &n),
        "JSON schema protocol list",
    )?);
    writes.push(edit_file(
        root.join("fuzz/Cargo.toml"),
        |text| add_fuzz_manifest(text, &n),
        "fuzz manifest",
    )?);
    writes.push(edit_file(
        root.join("crates/tpt-protocol-daemon/Cargo.toml"),
        |text| add_daemon_manifest(text, &n),
        "daemon dependency and feature",
    )?);
    writes.push(edit_file(
        root.join("crates/tpt-protocol-daemon/src/plugins/mod.rs"),
        |text| add_daemon_plugin(text, &n),
        "daemon plugin",
    )?);

    for write in &writes {
        println!(
            "{:<32} {}",
            write.why,
            write
                .path
                .strip_prefix(root)
                .unwrap_or(&write.path)
                .display()
        );
    }
    if dry_run {
        println!("(dry run: nothing written)");
        return Ok(());
    }
    for write in &writes {
        if let Some(parent) = write.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        fs::write(&write.path, &write.contents)
            .map_err(|e| format!("cannot write {}: {e}", write.path.display()))?;
    }
    println!();
    println!("Scaffolded {}.", n.crate_name);
    println!(
        "Next: replace the placeholder framing in crates/{}/src/lib.rs",
        n.crate_name
    );
    println!("and run `cargo test -p {}`.", n.crate_name);
    Ok(())
}

fn new_file(path: PathBuf, contents: String, why: &'static str) -> Write {
    Write {
        path,
        contents,
        why,
    }
}

/// Applies `edit` to an existing file. Files may use CRLF line endings (a Windows
/// checkout does), so the edit works on LF text and the original style is restored.
fn edit_file(
    path: PathBuf,
    edit: impl FnOnce(&str) -> Result<String, String>,
    why: &'static str,
) -> Result<Write, String> {
    let original =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let crlf = original.contains("\r\n");
    let lf = original.replace("\r\n", "\n");
    let edited = edit(&lf).map_err(|e| format!("{}: {e}", path.display()))?;
    let contents = if crlf {
        edited.replace('\n', "\r\n")
    } else {
        edited
    };
    Ok(Write {
        path,
        contents,
        why,
    })
}

/// Inserts `line` immediately before the first occurrence of `anchor`.
fn insert_before(text: &str, anchor: &str, line: &str) -> Result<String, String> {
    let at = text
        .find(anchor)
        .ok_or_else(|| format!("anchor not found: {:?}", anchor.trim()))?;
    Ok(format!("{}{}{}", &text[..at], line, &text[at..]))
}

/// Inserts `line` immediately after the first occurrence of `anchor`.
fn insert_after(text: &str, anchor: &str, line: &str) -> Result<String, String> {
    let at = text
        .find(anchor)
        .ok_or_else(|| format!("anchor not found: {:?}", anchor.trim()))?;
    let end = at + anchor.len();
    Ok(format!("{}{}{}", &text[..end], line, &text[end..]))
}

/// Inserts `line` before the first occurrence of `anchor` that comes after `from`.
fn insert_before_after(text: &str, from: &str, anchor: &str, line: &str) -> Result<String, String> {
    let start = text
        .find(from)
        .ok_or_else(|| format!("section not found: {:?}", from.trim()))?;
    let at = text[start..]
        .find(anchor)
        .map(|i| start + i)
        .ok_or_else(|| {
            format!(
                "anchor {:?} not found after {:?}",
                anchor.trim(),
                from.trim()
            )
        })?;
    Ok(format!("{}{}{}", &text[..at], line, &text[at..]))
}

fn add_source_protocol_variant(text: &str, n: &Names) -> Result<String, String> {
    // The enum: a new last variant, before the closing brace.
    let text = insert_before_after(
        text,
        "pub enum SourceProtocol {",
        "\n}",
        &format!("\n    {},", n.variant),
    )?;
    // as_str: a new arm, before the match's closing brace.
    let text = insert_before_after(
        &text,
        "pub fn as_str(&self) -> &'static str {",
        "\n        }\n",
        &format!("\n            Self::{} => \"{}\",", n.variant, n.name),
    )?;
    // from_str_inner: a new arm, before the wildcard.
    insert_before_after(
        &text,
        "fn from_str_inner(",
        "            _ => None,",
        &format!("            \"{}\" => Some(Self::{}),\n", n.name, n.variant),
    )
}

fn add_conformance_entry(text: &str, n: &Names) -> Result<String, String> {
    const HEADER: &str = "const ALL_PROTOCOLS: [SourceProtocol; ";
    let start = text.find(HEADER).ok_or("ALL_PROTOCOLS not found")?;
    let len_start = start + HEADER.len();
    let len_end = text[len_start..]
        .find(']')
        .map(|i| len_start + i)
        .ok_or("ALL_PROTOCOLS length not found")?;
    let count: usize = text[len_start..len_end]
        .parse()
        .map_err(|_| "ALL_PROTOCOLS length is not a number".to_string())?;
    let bumped = format!("{}{}{}", &text[..len_start], count + 1, &text[len_end..]);
    insert_before_after(
        &bumped,
        "const ALL_PROTOCOLS",
        "\n];",
        &format!("\n    SourceProtocol::{},", n.variant),
    )
}

fn add_cddl_protocol(text: &str, n: &Names) -> Result<String, String> {
    // The list ends at the blank line before `quality =`.
    let start = text
        .find("source-protocol =")
        .ok_or("source-protocol rule not found")?;
    let end = text[start..]
        .find("\n\n")
        .map(|i| start + i)
        .ok_or("end of the source-protocol rule not found")?;
    Ok(format!("{} / \"{}\"{}", &text[..end], n.name, &text[end..]))
}

fn add_json_schema_protocol(text: &str, n: &Names) -> Result<String, String> {
    let start = text
        .find("\"source_protocol\"")
        .ok_or("source_protocol not found")?;
    let enum_at = text[start..]
        .find("\"enum\": [")
        .map(|i| start + i)
        .ok_or("source_protocol enum not found")?;
    let close = text[enum_at..]
        .find("\n      ]")
        .map(|i| enum_at + i)
        .ok_or("end of the source_protocol enum not found")?;
    Ok(format!(
        "{}, \"{}\"{}",
        &text[..close],
        n.name,
        &text[close..]
    ))
}

fn add_fuzz_manifest(text: &str, n: &Names) -> Result<String, String> {
    let text = insert_after(
        text,
        "tpt-protocol-core = { path = \"../crates/tpt-protocol-core\" }\n",
        &format!(
            "tpt-protocol-{} = {{ path = \"../crates/{}\" }}\n",
            n.name, n.crate_name
        ),
    )?;
    Ok(format!(
        "{}\n[[bin]]\nname = \"{}\"\npath = \"fuzz_targets/{}.rs\"\ntest = false\ndoc = false\nbench = false\n",
        text.trim_end_matches('\n'),
        n.ident,
        n.ident
    ))
}

fn add_daemon_manifest(text: &str, n: &Names) -> Result<String, String> {
    // The dependency goes after the last protocol dependency (coap).
    let text = insert_after(
        text,
        "tpt-protocol-coap = { path = \"../tpt-protocol-coap\", optional = true }\n",
        &format!(
            "tpt-protocol-{} = {{ path = \"../{}\", optional = true }}\n",
            n.name, n.crate_name
        ),
    )?;
    // On by default, so `cargo build` includes the new protocol.
    let text = text.replacen("\"coap\",\n]", &format!("\"coap\", \"{}\",\n]", n.name), 1);
    // A feature of the same name.
    insert_after(
        &text,
        "coap = [\"dep:tpt-protocol-coap\"]\n",
        &format!("{} = [\"dep:tpt-protocol-{}\"]\n", n.name, n.name),
    )
}

fn add_daemon_plugin(text: &str, n: &Names) -> Result<String, String> {
    // The "no feature enabled" guard lists every feature.
    let text = insert_before(
        text,
        "    feature = \"coap\",\n)))]",
        &format!("    feature = \"{}\",\n", n.name),
    )?;
    // The protocol table: a new entry after the last one (coap), before the
    // closing brace of the def_protocol! invocation.
    let last = "SourceProtocol::Coap),\n}";
    let at = text
        .find(last)
        .ok_or("the last def_protocol entry (coap) was not found")?
        + "SourceProtocol::Coap),\n".len();
    let entry = format!(
        "    #[cfg(feature = \"{name}\")]\n    ({v}Plugin, \"{name}\", tpt_protocol_{id}::{v}Parser, tpt_protocol_{id}::{v}Normalizer, SourceProtocol::{v}),\n",
        name = n.name,
        v = n.variant,
        id = n.ident
    );
    Ok(format!("{}{}{}", &text[..at], entry, &text[at..]))
}

// --- New file templates -----------------------------------------------------

fn cargo_toml(n: &Names) -> String {
    format!(
        r#"[package]
# Set `publish` when the crate is ready for crates.io. Until then it stays private.
publish = false
name = "{crate_name}"
version = "0.1.0"
description = "{name} protocol implementation for the TPT Protocol suite"
keywords = ["tpt", "protocol", "{name}"]
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
tpt-protocol-core = {{ workspace = true }}
tpt-protocol-tup = {{ workspace = true }}

[dev-dependencies]
proptest = {{ workspace = true }}

[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]
"#,
        crate_name = n.crate_name,
        name = n.name,
    )
}

fn lib_rs(n: &Names) -> String {
    format!(
        r#"//! {name} protocol implementation for the TPT Protocol suite.
//!
//! The parser below accepts any non-empty frame and keeps its bytes. Replace it
//! with the protocol's real framing, and give the normalizer real fields.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::vec::Vec;

use tpt_protocol_core::{{DataNormalizer, Error, FrameParser, StateValidator}};
use tpt_protocol_tup::{{DataPoint, Quality, SourceProtocol, TupValue}};

/// A decoded {name} frame.
#[derive(Debug, Clone, PartialEq)]
pub struct {v}Frame {{
    pub raw: Vec<u8>,
}}

/// Parses {name} frames.
#[derive(Debug, Clone, Copy, Default)]
pub struct {v}Parser;

impl FrameParser for {v}Parser {{
    type Frame = {v}Frame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {{
        if bytes.is_empty() {{
            return Err(Error::Parse("{name} frame is empty".into()));
        }}
        Ok({v}Frame {{ raw: bytes.to_vec() }})
    }}
}}

/// Validates {name} connection state. There is no connection state yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct {v}Validator;

impl StateValidator for {v}Validator {{
    type State = ();

    fn validate(&self, _state: &Self::State) -> Result<(), Error> {{
        Ok(())
    }}
}}

/// Normalizes {name} frames into TUP data points.
#[derive(Debug, Clone, Copy, Default)]
pub struct {v}Normalizer;

impl DataNormalizer for {v}Normalizer {{
    type Frame = {v}Frame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {{
        if frame.raw.is_empty() {{
            return Err(Error::Normalize("{name} frame carries no data".into()));
        }}
        Ok(DataPoint::new(
            SourceProtocol::{v},
            "device",
            "point",
            0,
            TupValue::Bytes(frame.raw.clone()),
            Quality::Good,
        ))
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn parses_and_normalizes_a_frame() {{
        let frame = {v}Parser.parse(&[0x01, 0x02]).unwrap();
        assert_eq!(frame.raw, [0x01, 0x02]);
        let dp = {v}Normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::{v});
    }}

    #[test]
    fn empty_frame_is_rejected() {{
        assert!({v}Parser.parse(&[]).is_err());
    }}
}}

#[cfg(test)]
mod fuzz {{
    use super::*;
    use proptest::prelude::*;

    proptest! {{
        /// Random bytes must never panic the parser or the normalizer.
        #[test]
        fn random_input_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {{
            if let Ok(frame) = {v}Parser.parse(&bytes) {{
                let _ = {v}Normalizer.normalize(&frame);
            }}
        }}
    }}
}}
"#,
        name = n.name,
        v = n.variant,
    )
}

fn readme(n: &Names) -> String {
    format!(
        "# {crate_name}\n\n\
         {name} protocol implementation for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.\n\n\
         **Status: scaffolded.** The parser accepts any non-empty frame and the normalizer\n\
         reports the raw bytes. Replace both with the protocol's framing and fields.\n\n\
         ```bash\n\
         cargo run -p {crate_name} --example parse_{ident}\n\
         cargo test -p {crate_name}\n\
         ```\n\n\
         `#![no_std]`, and it depends only on `tpt-protocol-core` and `tpt-protocol-tup`.\n",
        crate_name = n.crate_name,
        name = n.name,
        ident = n.ident,
    )
}

fn example(n: &Names) -> String {
    format!(
        r#"//! Parse a {name} frame and normalize it. Replace the sample bytes with a real capture.
//!
//! Run with: `cargo run -p {crate_name} --example parse_{ident}`

use tpt_protocol_core::{{DataNormalizer, FrameParser}};
use tpt_protocol_{ident}::{{{v}Normalizer, {v}Parser}};

fn main() {{
    let bytes = [0x01, 0x02, 0x03];
    let shown: Vec<String> = bytes.iter().map(|b| format!("{{b:02X}}")).collect();
    println!("frame (hex): {{}}", shown.join(" "));

    let frame = {v}Parser.parse(&bytes).expect("a {name} frame parses");
    println!("parsed: {{frame:?}}");

    match {v}Normalizer.normalize(&frame) {{
        Ok(dp) => println!("normalized: {{}} {{}} = {{:?}}", dp.source_protocol, dp.point_id, dp.value),
        Err(e) => println!("no data point: {{e:?}}"),
    }}
}}
"#,
        name = n.name,
        crate_name = n.crate_name,
        ident = n.ident,
        v = n.variant,
    )
}

fn fuzz_target(n: &Names) -> String {
    format!(
        r#"#![no_main]
//! Fuzz target: `{v}Parser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_{ident}::{v}Parser;

fuzz_target!(|data: &[u8]| {{
    let _ = {v}Parser.parse(data);
    let _ = {v}Parser.try_parse(data);
}});
"#,
        ident = n.ident,
        v = n.variant,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_derived_consistently() {
        let n = names("matter-thread").unwrap();
        assert_eq!(n.crate_name, "tpt-protocol-matter-thread");
        assert_eq!(n.ident, "matter_thread");
        assert_eq!(n.variant, "MatterThread");
        assert_eq!(names("hl7v2").unwrap().variant, "Hl7v2");
    }

    #[test]
    fn invalid_names_are_refused() {
        for bad in [
            "",
            "Zigbee",
            "9lives",
            "two--dashes",
            "trailing-",
            "has space",
            "a_b",
        ] {
            assert!(names(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn source_protocol_edit_adds_enum_arm_and_name_tables() {
        let text = "pub enum SourceProtocol {\n    Modbus,\n    Coap,\n}\n\nimpl SourceProtocol {\n    pub fn as_str(&self) -> &'static str {\n        match self {\n            Self::Modbus => \"modbus\",\n            Self::Coap => \"coap\",\n        }\n    }\n\n    fn from_str_inner(s: &str) -> Option<Self> {\n        match s {\n            \"modbus\" => Some(Self::Modbus),\n            _ => None,\n        }\n    }\n}\n";
        let out = add_source_protocol_variant(text, &names("zigbee").unwrap()).unwrap();
        assert!(out.contains("    Coap,\n    Zigbee,\n}"));
        assert!(out.contains("            Self::Zigbee => \"zigbee\","));
        assert!(
            out.contains("            \"zigbee\" => Some(Self::Zigbee),\n            _ => None,")
        );
    }

    #[test]
    fn workspace_manifest_has_the_member_anchor() {
        // The real manifest must contain the anchor the scaffolder inserts before.
        let manifest =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml"))
                .unwrap()
                .replace("\r\n", "\n");
        let out = insert_before(
            &manifest,
            "    \"crates/tpt-protocol-daemon\",\n",
            "    \"crates/tpt-protocol-zigbee\",\n",
        );
        assert!(out.is_ok(), "{out:?}");
    }

    #[test]
    fn missing_anchors_are_errors_not_silent_skips() {
        assert!(insert_before("nothing here", "missing", "x").is_err());
        assert!(add_cddl_protocol("no rule", &names("x1").unwrap()).is_err());
    }

    #[test]
    fn cddl_and_json_lists_gain_the_name() {
        let cddl = "source-protocol = \"modbus\" / \"coap\"\n\nquality = \"good\"\n";
        let out = add_cddl_protocol(cddl, &names("zigbee").unwrap()).unwrap();
        assert!(out.contains("\"coap\" / \"zigbee\"\n\nquality"));

        let json = "    \"source_protocol\": {\n      \"type\": \"string\",\n      \"enum\": [\n        \"modbus\", \"coap\"\n      ]\n    }\n";
        let out = add_json_schema_protocol(json, &names("zigbee").unwrap()).unwrap();
        assert!(out.contains("\"coap\", \"zigbee\"\n      ]"));
    }

    #[test]
    fn conformance_array_grows_by_one() {
        let text = "const ALL_PROTOCOLS: [SourceProtocol; 2] = [\n    SourceProtocol::Modbus,\n    SourceProtocol::Coap,\n];\n";
        let out = add_conformance_entry(text, &names("zigbee").unwrap()).unwrap();
        assert!(out.starts_with("const ALL_PROTOCOLS: [SourceProtocol; 3] = ["));
        assert!(out.contains("SourceProtocol::Coap,\n    SourceProtocol::Zigbee,\n];"));
    }

    #[test]
    fn scaffold_writes_nothing_on_a_dry_run() {
        let dir = std::env::temp_dir().join(format!("xtask-dry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // With no anchors present the run fails before it could write anything.
        assert!(new_protocol(&dir, "zigbee", true).is_err());
        assert!(!dir.join("crates").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
