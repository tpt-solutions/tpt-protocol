//! Modbus TCP captures: replays the conversations between a client and a server
//! and turns each read response into points, as the active poller does.
//!
//! A response carries no start address, so each one is paired with its request by
//! transaction id. Requests and responses are kept in separate streams for each
//! TCP conversation, because the same bytes can be a request or a response: a
//! merged stream would misread them. The server port tells the two directions
//! apart, so `--pcap-port` is required.
//!
//! Only read requests (functions 3 and 4) produce points. A response whose request
//! is not in the capture, and an exception response, produce nothing.

use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};
use tpt_protocol_core::FrameParser;
use tpt_protocol_modbus::{ModbusFrame, ModbusParser};
use tpt_protocol_tup::DataPoint;

use crate::pcap::{self, Segment};
use crate::plugins::now_ms;
use crate::transport::{register_points, PollSpec};

/// One TCP conversation, identified by the client and server endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FlowKey {
    client_ip: [u8; 4],
    client_port: u16,
    server_ip: [u8; 4],
    server_port: u16,
}

#[derive(Default)]
struct Flow {
    requests: Vec<u8>,
    responses: Vec<u8>,
    /// Read requests seen, by transaction id, waiting for their response.
    pending: HashMap<u16, PollSpec>,
}

/// Replays the Modbus TCP conversations in `raw` (a pcap or pcapng capture) whose
/// server listens on `server_port`, and returns the points of their read responses.
pub fn replay(raw: &[u8], server_port: u16) -> Result<Vec<DataPoint>> {
    let segments = pcap::segments(raw).map_err(|e| anyhow!("pcap: {e}"))?;
    let mut flows: HashMap<FlowKey, Flow> = HashMap::new();
    let mut out = Vec::new();
    let requests = ModbusParser::new();
    let responses = ModbusParser::response();
    let mut unpaired = 0usize;

    for segment in segments {
        if !segment.tcp || segment.src_port == segment.dst_port {
            continue;
        }
        let Some((key, to_server)) = flow_of(&segment, server_port) else {
            continue;
        };
        let flow = flows.entry(key).or_default();
        if to_server {
            flow.requests.extend_from_slice(&segment.payload);
            drain_requests(&requests, &mut flow.requests, &mut flow.pending);
        } else {
            flow.responses.extend_from_slice(&segment.payload);
            unpaired += drain_responses(&responses, flow, &mut out);
        }
    }
    if unpaired > 0 {
        tracing::info!("pcap: {unpaired} responses had no request in the capture");
    }
    if out.is_empty() && flows.is_empty() {
        bail!("the capture holds no TCP conversation on port {server_port}");
    }
    Ok(out)
}

/// The conversation a segment belongs to, and whether it travels to the server.
/// Segments that touch neither endpoint's port are not Modbus traffic for this server.
fn flow_of(segment: &Segment, server_port: u16) -> Option<(FlowKey, bool)> {
    if segment.dst_port == server_port {
        Some((
            FlowKey {
                client_ip: segment.src_ip,
                client_port: segment.src_port,
                server_ip: segment.dst_ip,
                server_port,
            },
            true,
        ))
    } else if segment.src_port == server_port {
        Some((
            FlowKey {
                client_ip: segment.dst_ip,
                client_port: segment.dst_port,
                server_ip: segment.src_ip,
                server_port,
            },
            false,
        ))
    } else {
        None
    }
}

/// Parses every complete request in `buf` and remembers the read requests.
fn drain_requests(parser: &ModbusParser, buf: &mut Vec<u8>, pending: &mut HashMap<u16, PollSpec>) {
    loop {
        match parser.try_parse(buf) {
            Ok(Some((frame, used))) => {
                buf.drain(..used);
                let Some(tid) = frame.transaction_id else {
                    continue;
                };
                let (function, address, quantity) = match frame.frame {
                    ModbusFrame::ReadHoldingRegisters { address, quantity } => {
                        (3, address, quantity)
                    }
                    ModbusFrame::ReadInputRegisters { address, quantity } => (4, address, quantity),
                    _ => continue,
                };
                if let Ok(spec) = PollSpec::new(frame.slave_address, function, address, quantity) {
                    pending.insert(tid, spec);
                }
            }
            Ok(None) => return,
            Err(e) => {
                tracing::warn!("pcap: modbus request stream: {e:?}; resyncing");
                buf.clear();
                return;
            }
        }
    }
}

/// Parses every complete response in `flow`, pairs each with its request, and
/// appends the points. Returns the number of responses with no request.
fn drain_responses(parser: &ModbusParser, flow: &mut Flow, out: &mut Vec<DataPoint>) -> usize {
    let mut unpaired = 0;
    loop {
        match parser.try_parse(&flow.responses) {
            Ok(Some((frame, used))) => {
                flow.responses.drain(..used);
                let Some(tid) = frame.transaction_id else {
                    continue;
                };
                let Some(spec) = flow.pending.remove(&tid) else {
                    unpaired += 1;
                    continue;
                };
                match register_points(spec, &frame, now_ms()) {
                    Ok(points) => out.extend(points),
                    Err(e) => tracing::debug!("pcap: response {tid} is not data: {e}"),
                }
            }
            Ok(None) => return unpaired,
            Err(e) => {
                tracing::warn!("pcap: modbus response stream: {e:?}; resyncing");
                flow.responses.clear();
                return unpaired;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcap::tests::{classic, tcp_packet_between};

    const CLIENT: [u8; 4] = [10, 0, 0, 1];
    const OTHER_CLIENT: [u8; 4] = [10, 0, 0, 3];
    const SERVER: [u8; 4] = [10, 0, 0, 2];
    const PORT: u16 = 502;

    /// A Modbus TCP request (MBAP + read holding registers).
    fn request(tid: u16, unit: u8, start: u16, quantity: u16) -> Vec<u8> {
        let mut out = tid.to_be_bytes().to_vec();
        out.extend_from_slice(&[0, 0, 0, 6, unit, 3]);
        out.extend_from_slice(&start.to_be_bytes());
        out.extend_from_slice(&quantity.to_be_bytes());
        out
    }

    /// A read holding registers response with the given register values.
    fn response(tid: u16, unit: u8, values: &[u16]) -> Vec<u8> {
        let byte_count = (values.len() * 2) as u8;
        let length = 3 + u16::from(byte_count);
        let mut out = tid.to_be_bytes().to_vec();
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(&[unit, 3, byte_count]);
        for v in values {
            out.extend_from_slice(&v.to_be_bytes());
        }
        out
    }

    /// A request from `client` to the server.
    fn to_server(client: [u8; 4], client_port: u16, payload: &[u8]) -> Vec<u8> {
        tcp_packet_between(client, client_port, SERVER, PORT, payload)
    }

    /// A response from the server to `client`.
    fn to_client(client: [u8; 4], client_port: u16, payload: &[u8]) -> Vec<u8> {
        tcp_packet_between(SERVER, PORT, client, client_port, payload)
    }

    #[test]
    fn responses_are_paired_with_their_requests_by_transaction_id() {
        let capture = classic(&[
            to_server(CLIENT, 40000, &request(1, 1, 100, 3)),
            to_client(CLIENT, 40000, &response(1, 1, &[7, 8, 9])),
        ]);
        let points = replay(&capture, PORT).unwrap();
        let ids: Vec<&str> = points.iter().map(|p| p.point_id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "holding_register[100]",
                "holding_register[101]",
                "holding_register[102]"
            ]
        );
        assert_eq!(points[0].device_id, "modbus:1");
    }

    #[test]
    fn a_response_with_no_request_produces_nothing() {
        let capture = classic(&[to_client(CLIENT, 40000, &response(9, 1, &[1]))]);
        assert!(replay(&capture, PORT).unwrap().is_empty());
    }

    #[test]
    fn an_exception_response_produces_nothing() {
        // Function 0x83 with exception code 2 (illegal address).
        let mut exception = 1u16.to_be_bytes().to_vec();
        exception.extend_from_slice(&[0, 0, 0, 3, 1, 0x83, 2]);
        let capture = classic(&[
            to_server(CLIENT, 40000, &request(1, 1, 4000, 10)),
            to_client(CLIENT, 40000, &exception),
        ]);
        assert!(replay(&capture, PORT).unwrap().is_empty());
    }

    #[test]
    fn requests_and_responses_of_two_clients_do_not_mix() {
        let capture = classic(&[
            to_server(CLIENT, 40000, &request(1, 1, 10, 1)),
            to_server(OTHER_CLIENT, 40001, &request(1, 2, 20, 1)),
            to_client(OTHER_CLIENT, 40001, &response(1, 2, &[222])),
            to_client(CLIENT, 40000, &response(1, 1, &[111])),
        ]);
        let points = replay(&capture, PORT).unwrap();
        let by_device: Vec<(&str, &str)> = points
            .iter()
            .map(|p| (p.device_id.as_str(), p.point_id.as_str()))
            .collect();
        assert_eq!(
            by_device,
            [
                ("modbus:2", "holding_register[20]"),
                ("modbus:1", "holding_register[10]"),
            ]
        );
    }

    /// A real capture: pymodbus 3.6.9 client and server on loopback, recorded with
    /// tcpdump (see scripts/capture-modbus-loopback.sh). The client printed
    /// [1001, ..., 1009, 0] for registers 100-109, and an exception for address 4000.
    #[test]
    fn the_recorded_pymodbus_exchange_decodes_to_the_values_the_client_saw() {
        let capture = include_bytes!("../samples/captures/modbus-loopback.pcap");
        let points = replay(capture, 5020).unwrap();
        let values: Vec<i64> = points
            .iter()
            .map(|p| match p.value {
                tpt_protocol_tup::TupValue::Int(v) => v,
                _ => panic!("register values are integers"),
            })
            .collect();
        assert_eq!(
            values,
            [1001, 1002, 1003, 1004, 1005, 1006, 1007, 1008, 1009, 0]
        );
        assert_eq!(points[0].point_id, "holding_register[100]");
        assert_eq!(points[9].point_id, "holding_register[109]");
    }

    #[test]
    fn a_capture_without_the_port_is_an_error() {
        let capture = classic(&[tcp_packet_between(
            CLIENT,
            40000,
            SERVER,
            9999,
            &request(1, 1, 0, 1),
        )]);
        assert!(replay(&capture, PORT).is_err());
    }
}
