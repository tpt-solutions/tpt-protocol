//! OPC UA server (`--opcua-listen ADDR`, for example `127.0.0.1:4840`): a read-only
//! server for the points the daemon produces.
//!
//! What it does:
//! - OPC UA Binary over TCP (`opc.tcp://`): Hello/Acknowledge, OpenSecureChannel and
//!   CloseSecureChannel with the `SecurityPolicy#None` policy, and messages carrying the
//!   services a client needs to browse and read: GetEndpoints, CreateSession,
//!   ActivateSession, CloseSession, Browse and Read. Any other service gets a
//!   ServiceFault.
//! - Anonymous access only. There is no certificate, signature or encryption, so the
//!   server refuses to be the only protection on a non-loopback address: it logs a
//!   warning instead, and the default address is loopback.
//! - Address space: the Root and Objects folders, the namespace array (namespace 1 is
//!   `urn:tpt-protocol`), a `TPT` folder, and one variable per point, named
//!   `<device>/<point>` with the node id `ns=1;s=<device>/<point>`. Each variable holds
//!   the point's latest value and its time. Good quality is a good status; Uncertain
//!   and Stale are an uncertain status; Bad is a bad status.
//! - The engineering unit is not exposed, and there are no subscriptions, history,
//!   methods or writes. A client that reads is served the latest value.
//!
//! Framing and the service-message wrapper come from the `tpt-protocol-opcua` parser.
//! The request fields the server needs are read here, with a small reader.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, RwLock};
use tpt_protocol_core::FrameParser;
use tpt_protocol_opcua::{OpcuaFrame, OpcuaMessageType, OpcuaParser};
use tpt_protocol_tup::{DataPoint, Quality, TupValue};

const NAMESPACE_URI: &str = "urn:tpt-protocol";
const SECURITY_POLICY_NONE: &str = "http://opcfoundation.org/UA/SecurityPolicy#None";
const TRANSPORT_PROFILE: &str = "http://opcfoundation.org/UA-Profile/Transport/uatcp-uasc-uabinary";
const SESSION_TIMEOUT_MS: f64 = 3_600_000.0;
const CHANNEL_LIFETIME_MS: u32 = 3_600_000;
const MAX_MESSAGE: u32 = 16 * 1024 * 1024;
const BUFFER_SIZE: u32 = 65_536;
/// 100 ns ticks from 1601-01-01 to 1970-01-01: the OPC UA DateTime epoch offset.
const EPOCH_TICKS: i64 = 116_444_736_000_000_000;

// Encoding ids of the service request and response types (their DefaultBinary ids).
const SERVICE_FAULT: u32 = 397;
const GET_ENDPOINTS_REQUEST: u32 = 428;
const GET_ENDPOINTS_RESPONSE: u32 = 431;
const OPEN_SECURE_CHANNEL_RESPONSE: u32 = 449;
const CREATE_SESSION_REQUEST: u32 = 461;
const CREATE_SESSION_RESPONSE: u32 = 464;
const ACTIVATE_SESSION_REQUEST: u32 = 467;
const ACTIVATE_SESSION_RESPONSE: u32 = 470;
const CLOSE_SESSION_REQUEST: u32 = 473;
const CLOSE_SESSION_RESPONSE: u32 = 476;
const BROWSE_REQUEST: u32 = 527;
const BROWSE_RESPONSE: u32 = 530;
const READ_REQUEST: u32 = 631;
const READ_RESPONSE: u32 = 634;

// Status codes.
const STATUS_GOOD: u32 = 0;
const STATUS_UNCERTAIN: u32 = 0x4000_0000;
const STATUS_BAD: u32 = 0x8000_0000;
const STATUS_BAD_DECODING: u32 = 0x8007_0000;
const STATUS_BAD_NODE_ID_UNKNOWN: u32 = 0x8034_0000;
const STATUS_BAD_ATTRIBUTE_ID_INVALID: u32 = 0x8035_0000;
const STATUS_BAD_SERVICE_UNSUPPORTED: u32 = 0x800B_0000;

// Well-known nodes and types.
const NODE_ROOT: u32 = 84;
const NODE_OBJECTS: u32 = 85;
const NODE_NAMESPACE_ARRAY: u32 = 2255;
const REF_ORGANIZES: u32 = 35;
const TYPE_FOLDER: u32 = 61;
const TYPE_BASE_DATA_VARIABLE: u32 = 63;
const CLASS_OBJECT: u32 = 1;
const CLASS_VARIABLE: u32 = 2;

// Built-in type ids (the Variant type byte).
const VARIANT_BOOLEAN: u8 = 1;
const VARIANT_BYTE: u8 = 3;
const VARIANT_INT32: u8 = 6;
const VARIANT_UINT32: u8 = 7;
const VARIANT_INT64: u8 = 8;
const VARIANT_DOUBLE: u8 = 11;
const VARIANT_STRING: u8 = 12;
const VARIANT_NODE_ID: u8 = 17;
const VARIANT_BYTE_STRING: u8 = 15;
const VARIANT_QUALIFIED_NAME: u8 = 20;
const VARIANT_LOCALIZED_TEXT: u8 = 21;
const VARIANT_ARRAY: u8 = 0x80;

/// A node identifier: numeric or string, in a namespace.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NodeKey {
    Numeric(u16, u32),
    Text(u16, String),
}

/// One point as the address space holds it.
#[derive(Clone, Debug, PartialEq)]
struct PointNode {
    device: String,
    point: String,
    value: TupValue,
    timestamp: u64,
    quality: Quality,
}

/// The points the server exposes, keyed by `device/point`.
#[derive(Default)]
pub struct Space {
    points: BTreeMap<String, PointNode>,
}

/// Keeps `space` current with the points on `rx`. The latest value of each point wins.
async fn track_points(space: Arc<RwLock<Space>>, mut rx: broadcast::Receiver<DataPoint>) {
    loop {
        match rx.recv().await {
            Ok(dp) => {
                let key = format!("{}/{}", dp.device_id, dp.point_id);
                space.write().await.points.insert(
                    key,
                    PointNode {
                        device: dp.device_id,
                        point: dp.point_id,
                        value: dp.value,
                        timestamp: dp.timestamp,
                        quality: dp.quality,
                    },
                );
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("opcua: fell behind and skipped {n} points");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// Serves OPC UA clients on `addr` until the listener fails. Points come from `rx`.
pub async fn run(addr: String, rx: broadcast::Receiver<DataPoint>) -> Result<()> {
    if !is_loopback(&addr) {
        tracing::warn!(
            "opcua: {addr} is not a loopback address; the server has no authentication or encryption"
        );
    }
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("opcua: listening on opc.tcp://{addr}");
    let space = Arc::new(RwLock::new(Space::default()));
    tokio::spawn(track_points(space.clone(), rx));
    let endpoint = format!("opc.tcp://{addr}");
    loop {
        let (stream, peer) = listener.accept().await?;
        tracing::debug!("opcua: client {peer} connected");
        let space = space.clone();
        let endpoint = endpoint.clone();
        tokio::spawn(async move {
            if let Err(e) = serve(stream, space, endpoint).await {
                tracing::debug!("opcua: client {peer} closed: {e:#}");
            }
        });
    }
}

fn is_loopback(addr: &str) -> bool {
    let host = addr.rsplit_once(':').map(|(h, _)| h).unwrap_or(addr);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

/// Writes OPC UA Binary values.
#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i64(&mut self, v: i64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn f64(&mut self, v: f64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.i32(v.len() as i32);
        self.0.extend_from_slice(v);
        self
    }
    /// A null byte string or array (length -1).
    fn null(&mut self) -> &mut Self {
        self.i32(-1)
    }
    fn string(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes())
    }
    fn node_id(&mut self, key: &NodeKey) -> &mut Self {
        match key {
            NodeKey::Numeric(0, id) if *id <= 0xFF => {
                self.u8(0x00).u8(*id as u8);
            }
            NodeKey::Numeric(ns, id) if *ns <= 0xFF && *id <= 0xFFFF => {
                self.u8(0x01).u8(*ns as u8).u16(*id as u16);
            }
            NodeKey::Numeric(ns, id) => {
                self.u8(0x02).u16(*ns).u32(*id);
            }
            NodeKey::Text(ns, s) => {
                self.u8(0x03).u16(*ns).string(s);
            }
        }
        self
    }
    /// An extension object with no body: the null NodeId, then encoding 0.
    fn empty_extension(&mut self) -> &mut Self {
        self.node_id(&NodeKey::Numeric(0, 0)).u8(0)
    }
    /// A `DateTime` for the current time.
    fn now(&mut self) -> &mut Self {
        self.i64(date_time(now_ms()))
    }
    /// A `LocalizedText` with text and no locale.
    fn text(&mut self, v: &str) -> &mut Self {
        self.u8(0x02).string(v)
    }
    fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

/// Reads OPC UA Binary values. Each read fails on input that is too short.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        match self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.data.len())
        {
            Some(end) => {
                let out = &self.data[self.pos..end];
                self.pos = end;
                Ok(out)
            }
            None => bail!("OPC UA message is truncated"),
        }
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into()?))
    }
    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into()?))
    }
    fn bytes(&mut self) -> Result<Option<Vec<u8>>> {
        let len = self.i32()?;
        if len < 0 {
            return Ok(None);
        }
        Ok(Some(self.take(len as usize)?.to_vec()))
    }
    fn string(&mut self) -> Result<Option<String>> {
        match self.bytes()? {
            Some(b) => Ok(Some(String::from_utf8(b)?)),
            None => Ok(None),
        }
    }
    /// An array length. A null array reads as zero items.
    fn array_len(&mut self) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 {
            return Ok(0);
        }
        if n as usize > self.data.len() {
            bail!("OPC UA array of {n} items does not fit the message");
        }
        Ok(n as usize)
    }
    fn node_id(&mut self) -> Result<NodeKey> {
        let encoding = self.u8()?;
        Ok(match encoding & 0x0F {
            0x00 => NodeKey::Numeric(0, u32::from(self.u8()?)),
            0x01 => {
                let ns = u16::from(self.u8()?);
                NodeKey::Numeric(ns, u32::from(self.u16()?))
            }
            0x02 => {
                let ns = self.u16()?;
                NodeKey::Numeric(ns, self.u32()?)
            }
            0x03 => {
                let ns = self.u16()?;
                NodeKey::Text(ns, self.string()?.unwrap_or_default())
            }
            0x04 => {
                // GUID identifiers are not in the address space; read past them.
                let ns = self.u16()?;
                self.take(16)?;
                NodeKey::Numeric(ns, 0)
            }
            0x05 => {
                let ns = self.u16()?;
                let _ = self.bytes()?;
                NodeKey::Numeric(ns, 0)
            }
            other => bail!("OPC UA NodeId encoding {other} is not supported"),
        })
    }
    /// Skips an extension object, returning its body if it has one.
    fn extension_object(&mut self) -> Result<Option<Vec<u8>>> {
        let _type_id = self.node_id()?;
        match self.u8()? {
            0 => Ok(None),
            _ => self.bytes(),
        }
    }
    /// Reads a RequestHeader and returns its RequestHandle.
    fn request_header(&mut self) -> Result<u32> {
        let _auth_token = self.node_id()?;
        let _timestamp = self.i64()?;
        let handle = self.u32()?;
        let _return_diagnostics = self.u32()?;
        let _audit = self.string()?;
        let _timeout_hint = self.u32()?;
        let _additional = self.extension_object()?;
        Ok(handle)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The OPC UA DateTime (100 ns ticks since 1601) for a Unix time in milliseconds.
fn date_time(unix_ms: u64) -> i64 {
    EPOCH_TICKS + (unix_ms as i64) * 10_000
}

/// Writes a ResponseHeader: the time, the echoed request handle, and a status.
fn response_header(w: &mut Writer, handle: u32, status: u32) {
    w.now().u32(handle).u32(status);
    w.u8(0); // ServiceDiagnostics: a DiagnosticInfo with no fields
    w.null(); // StringTable
    w.empty_extension(); // AdditionalHeader
}

/// One message on the wire: its three-byte type, the chunk byte `F`, the size, the body.
fn envelope(kind: &[u8; 3], body: &[u8]) -> Vec<u8> {
    let size = (8 + body.len()) as u32;
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(kind);
    out.push(b'F');
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// A MSG reply: the secure channel header, the NodeId of the response's binary encoding
/// (namespace 0, four-byte form), then the response.
fn msg_reply(
    channel: u32,
    token: u32,
    sequence: u32,
    request_id: u32,
    type_id: u32,
    body: &[u8],
) -> Vec<u8> {
    let mut w = Writer::default();
    w.u32(channel).u32(token).u32(sequence).u32(request_id);
    w.u8(0x01).u8(0).u16(type_id as u16);
    w.0.extend_from_slice(body);
    envelope(b"MSG", &w.into_inner())
}

/// Writes the endpoint the server offers: anonymous, with no security.
fn endpoint_description(w: &mut Writer, endpoint: &str) {
    w.string(endpoint);
    // ApplicationDescription
    w.string("urn:tpt-protocol:daemon");
    w.string(NAMESPACE_URI);
    w.text("TPT Protocol daemon");
    w.u32(0); // ApplicationType: Server
    w.null(); // GatewayServerUri
    w.null(); // DiscoveryProfileUri
    w.null(); // DiscoveryUrls
    w.null(); // ServerCertificate
    w.u32(1); // SecurityMode: None
    w.string(SECURITY_POLICY_NONE);
    // UserIdentityTokens: one anonymous policy.
    w.u32(1);
    w.string("anonymous");
    w.u32(0); // UserTokenType: Anonymous
    w.null(); // IssuedTokenType
    w.null(); // IssuerEndpointUrl
    w.null(); // SecurityPolicyUri
    w.string(TRANSPORT_PROFILE);
    w.u8(0); // SecurityLevel
}

/// The node of a point, in namespace 1.
fn point_key(device: &str, point: &str) -> NodeKey {
    NodeKey::Text(1, format!("{device}/{point}"))
}

/// The built-in Variant type a value carries.
fn variant_type(value: &TupValue) -> u8 {
    match value {
        TupValue::Int(_) => VARIANT_INT64,
        TupValue::Float(_) => VARIANT_DOUBLE,
        TupValue::Bool(_) => VARIANT_BOOLEAN,
        TupValue::Text(_) => VARIANT_STRING,
        TupValue::Bytes(_) => VARIANT_BYTE_STRING,
    }
}

/// The status code for a point's quality.
fn status_of(quality: Quality) -> u32 {
    match quality {
        Quality::Good => STATUS_GOOD,
        Quality::Bad => STATUS_BAD,
        // Uncertain, Stale, and any quality added later: not known to be good.
        _ => STATUS_UNCERTAIN,
    }
}

/// A DataValue with a Variant that `fill` writes after its type byte.
fn variant_data_value(
    w: &mut Writer,
    variant: u8,
    source_time: Option<i64>,
    fill: impl FnOnce(&mut Writer),
) {
    let mask = 0x01 | if source_time.is_some() { 0x04 } else { 0x00 };
    w.u8(mask).u8(variant);
    fill(w);
    if let Some(time) = source_time {
        w.i64(time);
    }
}

/// A DataValue holding only a status code.
fn status_data_value(w: &mut Writer, status: u32) {
    w.u8(0x02).u32(status);
}

/// The DataValue of a point: its value, its source time, and a status when it is not good.
fn point_data_value(w: &mut Writer, node: &PointNode) {
    let status = status_of(node.quality);
    let mut mask = 0x01 | 0x04;
    if status != STATUS_GOOD {
        mask |= 0x02;
    }
    w.u8(mask);
    write_value(w, &node.value);
    if status != STATUS_GOOD {
        w.u32(status);
    }
    w.i64(date_time(node.timestamp));
}

/// A Variant holding a point value: its type byte, then the value.
fn write_value(w: &mut Writer, value: &TupValue) {
    w.u8(variant_type(value));
    match value {
        TupValue::Int(v) => {
            w.i64(*v);
        }
        TupValue::Float(v) => {
            w.f64(*v);
        }
        TupValue::Bool(v) => {
            w.u8(u8::from(*v));
        }
        TupValue::Text(v) => {
            w.string(v);
        }
        TupValue::Bytes(v) => {
            w.bytes(v);
        }
    }
}

/// Browse name, display name, node class and type definition of a known node.
fn describe(node: &NodeKey, space: &Space) -> Option<(String, String, u32, Option<u32>)> {
    Some(match node {
        NodeKey::Numeric(0, NODE_ROOT) => ("Root".into(), "Root".into(), CLASS_OBJECT, None),
        NodeKey::Numeric(0, NODE_OBJECTS) => (
            "Objects".into(),
            "Objects".into(),
            CLASS_OBJECT,
            Some(TYPE_FOLDER),
        ),
        NodeKey::Numeric(0, NODE_NAMESPACE_ARRAY) => (
            "NamespaceArray".into(),
            "NamespaceArray".into(),
            CLASS_VARIABLE,
            Some(TYPE_BASE_DATA_VARIABLE),
        ),
        NodeKey::Numeric(1, 0) => ("TPT".into(), "TPT".into(), CLASS_OBJECT, Some(TYPE_FOLDER)),
        NodeKey::Text(1, key) => {
            let point = space.points.get(key)?;
            (
                key.clone(),
                point.point.clone(),
                CLASS_VARIABLE,
                Some(TYPE_BASE_DATA_VARIABLE),
            )
        }
        _ => return None,
    })
}

/// The references a node has, filtered by browse direction. Only forward references
/// exist here, so an inverse browse returns none.
fn references_of(node: &NodeKey, direction: u32, space: &Space) -> Vec<(NodeKey, u32)> {
    if direction == 1 {
        return Vec::new();
    }
    match node {
        NodeKey::Numeric(0, NODE_ROOT) => vec![(NodeKey::Numeric(0, NODE_OBJECTS), REF_ORGANIZES)],
        NodeKey::Numeric(0, NODE_OBJECTS) => vec![(NodeKey::Numeric(1, 0), REF_ORGANIZES)],
        NodeKey::Numeric(1, 0) => space
            .points
            .values()
            .map(|p| (point_key(&p.device, &p.point), REF_ORGANIZES))
            .collect(),
        _ => Vec::new(),
    }
}

/// Answers one Browse operation (one BrowseDescription).
fn write_browse_result(w: &mut Writer, node: &NodeKey, direction: u32, space: &Space) {
    if describe(node, space).is_none() {
        w.u32(STATUS_BAD_NODE_ID_UNKNOWN);
        w.null(); // ContinuationPoint
        w.u32(0); // References
        return;
    }
    let refs = references_of(node, direction, space);
    w.u32(STATUS_GOOD);
    w.null(); // ContinuationPoint
    w.u32(refs.len() as u32);
    for (target, ref_type) in refs {
        let (browse_name, display_name, class, type_definition) =
            describe(&target, space).unwrap_or((String::new(), String::new(), 0, None));
        w.node_id(&NodeKey::Numeric(0, ref_type)); // ReferenceTypeId
        w.u8(1); // IsForward
        w.node_id(&target); // NodeId
        w.u16(1).string(&browse_name); // BrowseName: a QualifiedName in namespace 1
        w.text(&display_name); // DisplayName
        w.u32(class);
        w.node_id(&NodeKey::Numeric(0, type_definition.unwrap_or(0))); // TypeDefinition
    }
}

/// Writes the DataValue of one attribute of one node.
fn write_attribute(w: &mut Writer, node: &NodeKey, attribute: u32, space: &Space) {
    let Some((browse_name, display_name, class, _)) = describe(node, space) else {
        status_data_value(w, STATUS_BAD_NODE_ID_UNKNOWN);
        return;
    };
    // AttributeId values from the OPC UA specification: 1 NodeId, 2 NodeClass,
    // 3 BrowseName, 4 DisplayName, 13 Value, 14 DataType, 15 ValueRank,
    // 17 AccessLevel, 18 UserAccessLevel.
    match attribute {
        1 => variant_data_value(w, VARIANT_NODE_ID, None, |w| {
            w.node_id(node);
        }),
        2 => variant_data_value(w, VARIANT_UINT32, None, |w| {
            w.u32(class);
        }),
        3 => variant_data_value(w, VARIANT_QUALIFIED_NAME, None, |w| {
            w.u16(1).string(&browse_name);
        }),
        4 => variant_data_value(w, VARIANT_LOCALIZED_TEXT, None, |w| {
            w.text(&display_name);
        }),
        13 => match node {
            NodeKey::Numeric(0, NODE_NAMESPACE_ARRAY) => {
                let names = ["http://opcfoundation.org/UA/", NAMESPACE_URI];
                variant_data_value(w, VARIANT_ARRAY | VARIANT_STRING, None, |w| {
                    w.i32(names.len() as i32);
                    for name in names {
                        w.string(name);
                    }
                });
            }
            NodeKey::Text(1, key) => match space.points.get(key) {
                Some(point) => point_data_value(w, point),
                None => status_data_value(w, STATUS_BAD_NODE_ID_UNKNOWN),
            },
            _ => status_data_value(w, STATUS_BAD_ATTRIBUTE_ID_INVALID),
        },
        14 => {
            // DataType: the type of the value, or the base type for other nodes.
            let data_type = match node {
                NodeKey::Text(1, key) => space
                    .points
                    .get(key)
                    .map(|p| NodeKey::Numeric(0, data_type_node(&p.value)))
                    .unwrap_or(NodeKey::Numeric(0, TYPE_BASE_DATA_VARIABLE)),
                NodeKey::Numeric(0, NODE_NAMESPACE_ARRAY) => NodeKey::Numeric(0, 12),
                _ => NodeKey::Numeric(0, 0),
            };
            variant_data_value(w, VARIANT_NODE_ID, None, |w| {
                w.node_id(&data_type);
            });
        }
        15 => variant_data_value(w, VARIANT_INT32, None, |w| {
            w.i32(-1); // scalar
        }),
        17 | 18 => {
            let level = if class == CLASS_VARIABLE { 1 } else { 0 };
            variant_data_value(w, VARIANT_BYTE, None, |w| {
                w.u8(level);
            });
        }
        _ => status_data_value(w, STATUS_BAD_ATTRIBUTE_ID_INVALID),
    }
}

/// The DataType node of a value's Variant type (the OPC UA built-in type nodes).
fn data_type_node(value: &TupValue) -> u32 {
    match value {
        TupValue::Int(_) => 8,
        TupValue::Float(_) => 11,
        TupValue::Bool(_) => 1,
        TupValue::Text(_) => 12,
        TupValue::Bytes(_) => 15,
    }
}

/// The channel and session state of one client connection.
struct Connection {
    channel_id: u32,
    token_id: u32,
    next_sequence: u32,
    session_id: u32,
}

/// Answers one service request. `body` is the request, starting with its RequestHeader.
/// Returns the response type id and body.
fn handle_service(
    type_id: u32,
    body: &[u8],
    connection: &mut Connection,
    space: &Space,
    endpoint: &str,
) -> Result<(u32, Vec<u8>)> {
    let mut r = Reader::new(body);
    let mut w = Writer::default();
    match type_id {
        GET_ENDPOINTS_REQUEST => {
            let handle = r.request_header()?;
            response_header(&mut w, handle, STATUS_GOOD);
            w.u32(1); // Endpoints
            endpoint_description(&mut w, endpoint);
            Ok((GET_ENDPOINTS_RESPONSE, w.into_inner()))
        }
        CREATE_SESSION_REQUEST => {
            let handle = r.request_header()?;
            connection.session_id = connection.session_id.wrapping_add(1).max(1);
            let session = connection.session_id;
            response_header(&mut w, handle, STATUS_GOOD);
            w.node_id(&NodeKey::Numeric(1, session)); // SessionId
            w.node_id(&NodeKey::Numeric(1, session + 100_000)); // AuthenticationToken
            w.f64(SESSION_TIMEOUT_MS); // RevisedSessionTimeout
            w.bytes(&[0u8; 32]); // ServerNonce
            w.null(); // ServerCertificate
            w.u32(1); // ServerEndpoints
            endpoint_description(&mut w, endpoint);
            w.u32(0); // ServerSoftwareCertificates
            w.null(); // ServerSignature.Algorithm
            w.null(); // ServerSignature.Signature
            w.u32(MAX_MESSAGE); // MaxRequestMessageSize
            Ok((CREATE_SESSION_RESPONSE, w.into_inner()))
        }
        ACTIVATE_SESSION_REQUEST => {
            let handle = r.request_header()?;
            response_header(&mut w, handle, STATUS_GOOD);
            w.bytes(&[0u8; 32]); // ServerNonce
            w.u32(0); // Results
            w.u32(0); // DiagnosticInfos
            Ok((ACTIVATE_SESSION_RESPONSE, w.into_inner()))
        }
        CLOSE_SESSION_REQUEST => {
            let handle = r.request_header()?;
            response_header(&mut w, handle, STATUS_GOOD);
            Ok((CLOSE_SESSION_RESPONSE, w.into_inner()))
        }
        BROWSE_REQUEST => {
            let handle = r.request_header()?;
            let _view_id = r.node_id()?;
            let _view_time = r.i64()?;
            let _view_version = r.u32()?;
            let _max_references = r.u32()?;
            let count = r.array_len()?;
            response_header(&mut w, handle, STATUS_GOOD);
            w.u32(count as u32);
            for _ in 0..count {
                let node = r.node_id()?;
                let direction = r.u32()?;
                let _reference_type = r.node_id()?;
                let _include_subtypes = r.u8()?;
                let _class_mask = r.u32()?;
                let _result_mask = r.u32()?;
                write_browse_result(&mut w, &node, direction, space);
            }
            w.u32(0); // DiagnosticInfos
            Ok((BROWSE_RESPONSE, w.into_inner()))
        }
        READ_REQUEST => {
            let handle = r.request_header()?;
            let _max_age = r.f64()?;
            let _timestamps_to_return = r.u32()?;
            let count = r.array_len()?;
            response_header(&mut w, handle, STATUS_GOOD);
            w.u32(count as u32);
            for _ in 0..count {
                let node = r.node_id()?;
                let attribute = r.u32()?;
                let _index_range = r.string()?;
                let _data_encoding_ns = r.u16()?;
                let _data_encoding_name = r.string()?;
                write_attribute(&mut w, &node, attribute, space);
            }
            w.u32(0); // DiagnosticInfos
            Ok((READ_RESPONSE, w.into_inner()))
        }
        _ => {
            response_header(&mut w, 0, STATUS_BAD_SERVICE_UNSUPPORTED);
            Ok((SERVICE_FAULT, w.into_inner()))
        }
    }
}

/// Answers an OpenSecureChannel request with the `SecurityPolicy#None` policy. The body
/// is the asymmetric security header, the sequence header, then the request.
fn open_secure_channel(body: &[u8], connection: &mut Connection) -> Result<Vec<u8>> {
    let mut r = Reader::new(body);
    let _requested_channel = r.u32()?;
    let _policy = r.string()?;
    let _sender_certificate = r.bytes()?;
    let _receiver_thumbprint = r.bytes()?;
    let _sequence = r.u32()?;
    let request_id = r.u32()?;
    // The message body starts with the NodeId of the request's binary encoding, then the
    // request itself. (It is not an extension object: there is no encoding byte or length.)
    let _request_type_id = r.node_id()?;
    let handle = r.request_header()?;
    let _client_protocol_version = r.u32()?;
    let _request_type = r.u32()?;
    let _security_mode = r.u32()?;
    let _client_nonce = r.bytes()?;
    let _requested_lifetime = r.u32()?;

    connection.channel_id = connection.channel_id.max(1);
    connection.token_id = connection.token_id.wrapping_add(1).max(1);

    let mut w = Writer::default();
    // Asymmetric security header, with no certificates.
    w.u32(connection.channel_id);
    w.string(SECURITY_POLICY_NONE);
    w.null(); // SenderCertificate
    w.null(); // ReceiverCertificateThumbprint
              // Sequence header.
    let sequence = connection.next_sequence;
    connection.next_sequence += 1;
    w.u32(sequence).u32(request_id);
    // The OpenSecureChannelResponse: its encoding NodeId, then the response.
    let mut body = Writer::default();
    response_header(&mut body, handle, STATUS_GOOD);
    body.u32(0); // ServerProtocolVersion
    body.u32(connection.channel_id)
        .u32(connection.token_id)
        .i64(date_time(now_ms()))
        .u32(CHANNEL_LIFETIME_MS);
    body.bytes(&[0u8; 32]); // ServerNonce
    w.node_id(&NodeKey::Numeric(0, OPEN_SECURE_CHANNEL_RESPONSE));
    w.0.extend_from_slice(&body.into_inner());
    Ok(envelope(b"OPN", &w.into_inner()))
}

/// Serves one client connection until it closes or breaks the protocol.
async fn serve(mut stream: TcpStream, space: Arc<RwLock<Space>>, endpoint: String) -> Result<()> {
    let parser = OpcuaParser::new();
    let mut buffer: Vec<u8> = Vec::new();
    let mut connection = Connection {
        channel_id: 0,
        token_id: 0,
        next_sequence: 1,
        session_id: 0,
    };
    loop {
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..n]);
        loop {
            let (frame, used) = match parser.try_parse(&buffer) {
                Ok(Some(found)) => found,
                Ok(None) => break,
                Err(e) => bail!("malformed OPC UA message: {e:?}"),
            };
            buffer.drain(..used);
            let replies = {
                let space = space.read().await;
                respond(&frame, &mut connection, &space, &endpoint)?
            };
            let Some(replies) = replies else {
                return Ok(()); // the client closed the secure channel
            };
            for reply in replies {
                stream.write_all(&reply).await?;
            }
        }
    }
}

/// The replies to one message. `None` when the client closes the secure channel.
fn respond(
    frame: &OpcuaFrame,
    connection: &mut Connection,
    space: &Space,
    endpoint: &str,
) -> Result<Option<Vec<Vec<u8>>>> {
    match frame.message_type {
        OpcuaMessageType::Hello => {
            let mut w = Writer::default();
            w.u32(0) // ProtocolVersion
                .u32(BUFFER_SIZE) // ReceiveBufferSize
                .u32(BUFFER_SIZE) // SendBufferSize
                .u32(MAX_MESSAGE) // MaxMessageSize
                .u32(0); // MaxChunkCount
            Ok(Some(vec![envelope(b"ACK", &w.into_inner())]))
        }
        OpcuaMessageType::OpenSecureChannel => {
            let reply = open_secure_channel(&frame.payload, connection)?;
            Ok(Some(vec![reply]))
        }
        OpcuaMessageType::CloseSecureChannel => Ok(None),
        OpcuaMessageType::Message => {
            let Some(type_id) = frame.service_id else {
                bail!("OPC UA message without a service type id");
            };
            let (reply_type, reply_body) =
                match handle_service(type_id, &frame.payload, connection, space, endpoint) {
                    Ok(answer) => answer,
                    Err(_) => {
                        // A request that does not decode gets a fault, not a dropped channel.
                        let mut w = Writer::default();
                        response_header(&mut w, 0, STATUS_BAD_DECODING);
                        (SERVICE_FAULT, w.into_inner())
                    }
                };
            let sequence = connection.next_sequence;
            connection.next_sequence += 1;
            Ok(Some(vec![msg_reply(
                connection.channel_id,
                connection.token_id,
                sequence,
                frame.request_id,
                reply_type,
                &reply_body,
            )]))
        }
        OpcuaMessageType::Error | OpcuaMessageType::Acknowledge => {
            bail!("unexpected OPC UA message {}", frame.message_type)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_protocol_tup::SourceProtocol;

    fn connection() -> Connection {
        Connection {
            channel_id: 1,
            token_id: 1,
            next_sequence: 1,
            session_id: 0,
        }
    }

    fn point(space: &mut Space, device: &str, point: &str, value: TupValue, quality: Quality) {
        let dp = DataPoint::new(
            SourceProtocol::Nmea,
            device,
            point,
            1_753_900_000_000,
            value,
            quality,
        );
        space.points.insert(
            format!("{}/{}", dp.device_id, dp.point_id),
            PointNode {
                device: dp.device_id,
                point: dp.point_id,
                value: dp.value,
                timestamp: dp.timestamp,
                quality: dp.quality,
            },
        );
    }

    #[test]
    fn node_ids_round_trip_in_their_compact_forms() {
        for key in [
            NodeKey::Numeric(0, 84),
            NodeKey::Numeric(0, 2255),
            NodeKey::Numeric(1, 7),
            NodeKey::Numeric(300, 70_000),
            NodeKey::Text(1, "modbus:1/holding_register[100]".into()),
        ] {
            let mut w = Writer::default();
            w.node_id(&key);
            let bytes = w.into_inner();
            let mut r = Reader::new(&bytes);
            assert_eq!(r.node_id().unwrap(), key);
            assert_eq!(r.pos, bytes.len());
        }
    }

    #[test]
    fn the_reader_refuses_truncated_input_and_reads_null_as_empty() {
        assert!(Reader::new(&[0x01, 0x00]).u32().is_err());
        assert!(Reader::new(&[0x05, 0x00, 0x00, 0x00, 0x61])
            .string()
            .is_err());
        let mut w = Writer::default();
        w.null().i32(-1).string("x");
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.string().unwrap(), None);
        assert_eq!(r.array_len().unwrap(), 0);
        assert_eq!(r.string().unwrap().as_deref(), Some("x"));
    }

    #[test]
    fn date_times_count_from_1601() {
        // The Unix epoch is 11644473600 seconds after 1601.
        assert_eq!(date_time(0), 116_444_736_000_000_000);
        assert_eq!(date_time(1_000), 116_444_736_000_000_000 + 10_000_000);
    }

    #[test]
    fn the_loopback_check_recognises_local_addresses() {
        assert!(is_loopback("127.0.0.1:4840"));
        assert!(is_loopback("localhost:4840"));
        assert!(is_loopback("[::1]:4840"));
        assert!(!is_loopback("0.0.0.0:4840"));
        assert!(!is_loopback("192.168.1.10:4840"));
    }

    #[test]
    fn get_endpoints_answers_with_the_header_and_one_endpoint() {
        let mut w = Writer::default();
        // RequestHeader: no token, handle 9, no additional header.
        w.node_id(&NodeKey::Numeric(0, 0))
            .i64(0)
            .u32(9)
            .u32(0)
            .null()
            .u32(0)
            .empty_extension();
        w.null().u32(0); // EndpointUrl (null), LocaleIds
        w.u32(0); // ProfileUris
        let space = Space::default();
        let (type_id, body) = handle_service(
            GET_ENDPOINTS_REQUEST,
            &w.into_inner(),
            &mut connection(),
            &space,
            "opc.tcp://127.0.0.1:4840",
        )
        .unwrap();
        assert_eq!(type_id, GET_ENDPOINTS_RESPONSE);
        let mut r = Reader::new(&body);
        let _time = r.i64().unwrap();
        assert_eq!(r.u32().unwrap(), 9, "the handle is echoed");
        assert_eq!(r.u32().unwrap(), STATUS_GOOD);
        assert_eq!(r.u8().unwrap(), 0, "no diagnostics");
        assert_eq!(r.i32().unwrap(), -1, "no string table");
        assert_eq!(r.node_id().unwrap(), NodeKey::Numeric(0, 0));
        assert_eq!(r.u8().unwrap(), 0);
        assert_eq!(r.u32().unwrap(), 1, "one endpoint");
        assert_eq!(
            r.string().unwrap().as_deref(),
            Some("opc.tcp://127.0.0.1:4840")
        );
    }

    #[test]
    fn an_unknown_service_gets_a_service_fault() {
        let (type_id, body) =
            handle_service(9999, &[0; 40], &mut connection(), &Space::default(), "x").unwrap();
        assert_eq!(type_id, SERVICE_FAULT);
        let mut r = Reader::new(&body);
        let _time = r.i64().unwrap();
        let _handle = r.u32().unwrap();
        assert_eq!(r.u32().unwrap(), STATUS_BAD_SERVICE_UNSUPPORTED);
    }

    #[test]
    fn a_point_is_a_variable_holding_its_latest_value() {
        let mut space = Space::default();
        point(
            &mut space,
            "nmea:GP",
            "GGA.latitude",
            TupValue::Float(48.1173),
            Quality::Good,
        );
        let key = point_key("nmea:GP", "GGA.latitude");
        assert!(describe(&key, &space).is_some());
        assert!(describe(&point_key("nmea:GP", "missing"), &space).is_none());

        let mut w = Writer::default();
        write_attribute(&mut w, &key, 13, &space);
        let bytes = w.into_inner();
        // mask (value and source time), Double variant, then the value.
        assert_eq!(bytes[0], 0x05);
        assert_eq!(bytes[1], VARIANT_DOUBLE);
        assert_eq!(
            f64::from_le_bytes(bytes[2..10].try_into().unwrap()),
            48.1173
        );
    }

    #[test]
    fn a_stale_point_carries_an_uncertain_status() {
        let mut space = Space::default();
        point(&mut space, "d", "p", TupValue::Int(5), Quality::Stale);
        let mut w = Writer::default();
        write_attribute(&mut w, &point_key("d", "p"), 13, &space);
        let bytes = w.into_inner();
        assert_eq!(bytes[0], 0x07, "value, status and source time");
        // Variant Int64 (8) with the value, then the status code.
        assert_eq!(bytes[1], VARIANT_INT64);
        let status_at = 2 + 8;
        assert_eq!(
            u32::from_le_bytes(bytes[status_at..status_at + 4].try_into().unwrap()),
            STATUS_UNCERTAIN
        );
    }
}
