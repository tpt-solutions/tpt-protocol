//! Reads packet captures (classic pcap and pcapng) and returns the TCP and UDP
//! payloads inside them, in capture order, so a protocol parser can replay them.
//!
//! Supported: classic pcap and pcapng, with Ethernet II (one 802.1Q tag allowed)
//! or raw IP link types, and IPv4 only. Not supported, and skipped: IPv6, IPv4
//! fragments, and other link types (an error when a whole file uses one). TCP
//! payloads are not reassembled: segments are taken in capture order, with no
//! reordering and no gap filling. Use the port filter to keep one conversation.

const LINKTYPE_ETHERNET: u32 = 1;
const LINKTYPE_RAW_A: u32 = 12;
const LINKTYPE_RAW: u32 = 101;

const PCAP_MAGIC_MICROS: [u8; 4] = [0xA1, 0xB2, 0xC3, 0xD4];
const PCAP_MAGIC_NANOS: [u8; 4] = [0xA1, 0xB2, 0x3C, 0x4D];
const PCAPNG_SECTION: [u8; 4] = [0x0A, 0x0D, 0x0D, 0x0A];
const PCAPNG_BYTE_ORDER: u32 = 0x1A2B_3C4D;

/// Returns the TCP and UDP payloads of `bytes`, a pcap or pcapng file. `port`, when
/// given, keeps only segments whose source or destination port matches. Packets
/// without a payload are dropped.
pub fn payloads(bytes: &[u8], port: Option<u16>) -> Result<Vec<Vec<u8>>, String> {
    Ok(segments(bytes)?
        .into_iter()
        .filter(|s| port.is_none_or(|p| s.src_port == p || s.dst_port == p))
        .map(|s| s.payload)
        .collect())
}

/// One TCP or UDP segment with its addresses and its payload.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub src_ip: [u8; 4],
    pub src_port: u16,
    pub dst_ip: [u8; 4],
    pub dst_port: u16,
    /// True for TCP, false for UDP.
    pub tcp: bool,
    pub payload: Vec<u8>,
}

/// Every TCP and UDP segment in `bytes` that carries a payload, in capture order.
pub fn segments(bytes: &[u8]) -> Result<Vec<Segment>, String> {
    let packets = if bytes.starts_with(&PCAPNG_SECTION) {
        read_pcapng(bytes)?
    } else {
        read_classic(bytes)?
    };
    let mut out = Vec::new();
    for (linktype, packet) in packets {
        if !matches!(linktype, LINKTYPE_ETHERNET | LINKTYPE_RAW_A | LINKTYPE_RAW) {
            return Err(format!(
                "link type {linktype} is not supported (Ethernet and raw IP are)"
            ));
        }
        if let Some(segment) = ip_segment(linktype, packet) {
            out.push(segment);
        }
    }
    Ok(out)
}

/// A packet as `(link type, bytes)`. The bytes borrow from the file.
type Packet<'a> = (u32, &'a [u8]);

fn word(b: &[u8], big: bool) -> u32 {
    let w = [b[0], b[1], b[2], b[3]];
    if big {
        u32::from_be_bytes(w)
    } else {
        u32::from_le_bytes(w)
    }
}

fn read_classic(b: &[u8]) -> Result<Vec<Packet<'_>>, String> {
    if b.len() < 24 {
        return Err("the pcap header is truncated".into());
    }
    let magic = [b[0], b[1], b[2], b[3]];
    let big = if magic == PCAP_MAGIC_MICROS || magic == PCAP_MAGIC_NANOS {
        true
    } else if magic
        == [
            PCAP_MAGIC_MICROS[3],
            PCAP_MAGIC_MICROS[2],
            PCAP_MAGIC_MICROS[1],
            PCAP_MAGIC_MICROS[0],
        ]
        || magic
            == [
                PCAP_MAGIC_NANOS[3],
                PCAP_MAGIC_NANOS[2],
                PCAP_MAGIC_NANOS[1],
                PCAP_MAGIC_NANOS[0],
            ]
    {
        false
    } else {
        return Err("not a pcap or pcapng file".into());
    };
    let linktype = word(&b[20..24], big);
    let mut out = Vec::new();
    let mut pos = 24;
    while pos < b.len() {
        if pos + 16 > b.len() {
            return Err(format!("a packet header at byte {pos} is truncated"));
        }
        let captured = word(&b[pos + 8..pos + 12], big) as usize;
        let start = pos + 16;
        let end = start
            .checked_add(captured)
            .filter(|&end| end <= b.len())
            .ok_or_else(|| format!("the packet at byte {pos} is truncated"))?;
        out.push((linktype, &b[start..end]));
        pos = end;
    }
    Ok(out)
}

fn read_pcapng(b: &[u8]) -> Result<Vec<Packet<'_>>, String> {
    let mut big = false;
    // Link type of each interface, in the order the interface blocks declare them.
    let mut links: Vec<u32> = Vec::new();
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < b.len() {
        if pos + 12 > b.len() {
            return Err(format!("a block at byte {pos} is truncated"));
        }
        if b[pos..pos + 4] == PCAPNG_SECTION {
            // A section header sets the byte order for everything after it.
            let order = word(&b[pos + 8..pos + 12], false);
            big = match order {
                PCAPNG_BYTE_ORDER => false,
                _ if order.swap_bytes() == PCAPNG_BYTE_ORDER => true,
                _ => return Err("the pcapng byte-order magic is wrong".into()),
            };
            links.clear();
        }
        let block_type = word(&b[pos..pos + 4], big);
        let total = word(&b[pos + 4..pos + 8], big) as usize;
        if total < 12 || total % 4 != 0 || pos.checked_add(total).is_none_or(|e| e > b.len()) {
            return Err(format!("the block at byte {pos} has a bad length"));
        }
        let body = &b[pos + 8..pos + total - 4];
        match block_type {
            // Interface description: the link type is the first 16-bit field.
            1 => {
                if body.len() < 4 {
                    return Err("an interface block is truncated".into());
                }
                let link = if big {
                    u16::from_be_bytes([body[0], body[1]])
                } else {
                    u16::from_le_bytes([body[0], body[1]])
                };
                links.push(u32::from(link));
            }
            // Enhanced packet block: interface id, timestamp, captured length, data.
            6 => {
                if body.len() < 20 {
                    return Err("an enhanced packet block is truncated".into());
                }
                let iface = word(&body[0..4], big) as usize;
                let captured = word(&body[12..16], big) as usize;
                let link = *links
                    .get(iface)
                    .ok_or("a packet names an interface that was not declared")?;
                let data = body
                    .get(20..20 + captured)
                    .ok_or("an enhanced packet block is truncated")?;
                out.push((link, data));
            }
            // Simple packet block: original length, then the data.
            3 => {
                let link = *links.first().ok_or("a packet came before any interface")?;
                let original = word(&body[0..4], big) as usize;
                let data_all = &body[4..];
                let data = &data_all[..original.min(data_all.len())];
                out.push((link, data));
            }
            _ => {}
        }
        pos += total;
    }
    Ok(out)
}

/// The TCP or UDP payload of one IPv4 packet, or `None` when there is none to keep.
fn ip_segment(linktype: u32, packet: &[u8]) -> Option<Segment> {
    let ip = if linktype == LINKTYPE_RAW || linktype == LINKTYPE_RAW_A {
        packet
    } else {
        if packet.len() < 14 {
            return None;
        }
        let mut ethertype = u16::from_be_bytes([packet[12], packet[13]]);
        let mut offset = 14;
        if ethertype == 0x8100 {
            if packet.len() < 18 {
                return None;
            }
            ethertype = u16::from_be_bytes([packet[16], packet[17]]);
            offset = 18;
        }
        if ethertype != 0x0800 {
            return None;
        }
        &packet[offset..]
    };

    // IPv4 header.
    if ip.len() < 20 || ip[0] >> 4 != 4 {
        return None;
    }
    let header = usize::from(ip[0] & 0x0F) * 4;
    if header < 20 || ip.len() < header {
        return None;
    }
    let total = usize::from(u16::from_be_bytes([ip[2], ip[3]])).min(ip.len());
    if total < header {
        return None;
    }
    // More-fragments flag or a non-zero offset: the payload is only part of a datagram.
    if u16::from_be_bytes([ip[6], ip[7]]) & 0x3FFF != 0 {
        return None;
    }
    let segment = &ip[header..total];

    let (src_port, dst_port, payload, tcp) = match ip[9] {
        // TCP: the data offset in the high nibble of byte 12 gives the header length.
        6 => {
            if segment.len() < 20 {
                return None;
            }
            let data_offset = usize::from(segment[12] >> 4) * 4;
            if data_offset < 20 || segment.len() < data_offset {
                return None;
            }
            (
                u16::from_be_bytes([segment[0], segment[1]]),
                u16::from_be_bytes([segment[2], segment[3]]),
                &segment[data_offset..],
                true,
            )
        }
        // UDP: a fixed 8-byte header.
        17 => {
            if segment.len() < 8 {
                return None;
            }
            (
                u16::from_be_bytes([segment[0], segment[1]]),
                u16::from_be_bytes([segment[2], segment[3]]),
                &segment[8..],
                false,
            )
        }
        _ => return None,
    };

    if payload.is_empty() {
        return None;
    }
    Some(Segment {
        src_ip: [ip[12], ip[13], ip[14], ip[15]],
        src_port,
        dst_ip: [ip[16], ip[17], ip[18], ip[19]],
        dst_port,
        tcp,
        payload: payload.to_vec(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// One Ethernet II / IPv4 / TCP packet from 10.0.0.1 to 10.0.0.2 carrying `payload`.
    pub(crate) fn tcp_packet(src_port: u16, dst_port: u16, payload: &[u8]) -> Vec<u8> {
        tcp_packet_between([10, 0, 0, 1], src_port, [10, 0, 0, 2], dst_port, payload)
    }

    /// One Ethernet II / IPv4 / TCP packet between the given endpoints.
    pub(crate) fn tcp_packet_between(
        src_ip: [u8; 4],
        src_port: u16,
        dst_ip: [u8; 4],
        dst_port: u16,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut p = vec![0u8; 12];
        p.extend_from_slice(&[0x08, 0x00]); // IPv4
        let total = 20 + 20 + payload.len();
        let mut ip = vec![
            0x45,
            0,
            (total >> 8) as u8,
            total as u8,
            0,
            0,
            0x40,
            0,
            64,
            6,
            0,
            0,
        ];
        ip.extend_from_slice(&src_ip);
        ip.extend_from_slice(&dst_ip);
        p.extend_from_slice(&ip);
        let mut tcp = Vec::new();
        tcp.extend_from_slice(&src_port.to_be_bytes());
        tcp.extend_from_slice(&dst_port.to_be_bytes());
        tcp.extend_from_slice(&[0; 8]); // seq, ack
        tcp.extend_from_slice(&[0x50, 0x18, 0, 0, 0, 0, 0, 0]); // offset 5, PSH+ACK
        p.extend_from_slice(&tcp);
        p.extend_from_slice(payload);
        p
    }

    pub(crate) fn classic(packets: &[Vec<u8>]) -> Vec<u8> {
        // Little-endian file: the magic is stored as D4 C3 B2 A1.
        let mut f = vec![0xD4, 0xC3, 0xB2, 0xA1];
        f.extend_from_slice(&[2, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0, 0]);
        f.extend_from_slice(&LINKTYPE_ETHERNET.to_le_bytes());
        for p in packets {
            f.extend_from_slice(&[0; 8]); // timestamp
            f.extend_from_slice(&(p.len() as u32).to_le_bytes());
            f.extend_from_slice(&(p.len() as u32).to_le_bytes());
            f.extend_from_slice(p);
        }
        f
    }

    fn pad4(v: &mut Vec<u8>) {
        while v.len() % 4 != 0 {
            v.push(0);
        }
    }

    fn pcapng(packets: &[Vec<u8>]) -> Vec<u8> {
        let mut f = Vec::new();
        // Section header, little-endian, no options.
        f.extend_from_slice(&PCAPNG_SECTION);
        f.extend_from_slice(&28u32.to_le_bytes());
        f.extend_from_slice(&PCAPNG_BYTE_ORDER.to_le_bytes());
        // Version 1.0 and an unspecified section length (8 bytes).
        f.extend_from_slice(&[1, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        f.extend_from_slice(&28u32.to_le_bytes());
        // Interface description: Ethernet, no options. Reserved (2), snaplen (4).
        f.extend_from_slice(&1u32.to_le_bytes());
        f.extend_from_slice(&20u32.to_le_bytes());
        f.extend_from_slice(&(LINKTYPE_ETHERNET as u16).to_le_bytes());
        f.extend_from_slice(&[0, 0, 0xFF, 0xFF, 0, 0]);
        f.extend_from_slice(&20u32.to_le_bytes());
        for p in packets {
            let mut body = Vec::new();
            body.extend_from_slice(&0u32.to_le_bytes()); // interface 0
            body.extend_from_slice(&[0; 8]); // timestamp
            body.extend_from_slice(&(p.len() as u32).to_le_bytes());
            body.extend_from_slice(&(p.len() as u32).to_le_bytes());
            body.extend_from_slice(p);
            pad4(&mut body);
            let total = (body.len() + 12) as u32;
            f.extend_from_slice(&6u32.to_le_bytes());
            f.extend_from_slice(&total.to_le_bytes());
            f.extend_from_slice(&body);
            f.extend_from_slice(&total.to_le_bytes());
        }
        f
    }

    pub(crate) const MODBUS_REQUEST: [u8; 12] = [0, 1, 0, 0, 0, 6, 1, 3, 0, 0, 0, 10];

    #[test]
    fn classic_pcap_yields_the_tcp_payload() {
        let file = classic(&[tcp_packet(40000, 502, &MODBUS_REQUEST)]);
        assert_eq!(
            payloads(&file, None).unwrap(),
            vec![MODBUS_REQUEST.to_vec()]
        );
    }

    #[test]
    fn pcapng_yields_the_same_payload() {
        let file = pcapng(&[tcp_packet(40000, 502, &MODBUS_REQUEST)]);
        assert_eq!(
            payloads(&file, None).unwrap(),
            vec![MODBUS_REQUEST.to_vec()]
        );
    }

    #[test]
    fn the_port_filter_keeps_one_conversation() {
        let file = classic(&[
            tcp_packet(40000, 502, &MODBUS_REQUEST),
            tcp_packet(40001, 9999, b"other"),
            tcp_packet(502, 40000, b"reply"),
        ]);
        let kept = payloads(&file, Some(502)).unwrap();
        assert_eq!(kept, vec![MODBUS_REQUEST.to_vec(), b"reply".to_vec()]);
    }

    #[test]
    fn packets_without_payload_and_ipv6_are_skipped() {
        let mut ipv6 = tcp_packet(1, 502, b"x");
        ipv6[12] = 0x86;
        ipv6[13] = 0xDD;
        let empty = tcp_packet(40000, 502, &[]);
        let file = classic(&[ipv6, empty, tcp_packet(40000, 502, b"ok")]);
        assert_eq!(payloads(&file, None).unwrap(), vec![b"ok".to_vec()]);
    }

    #[test]
    fn fragments_are_skipped() {
        let mut fragment = tcp_packet(40000, 502, b"part");
        fragment[14 + 6] = 0x20; // more-fragments flag
        let file = classic(&[fragment]);
        assert!(payloads(&file, None).unwrap().is_empty());
    }

    #[test]
    fn a_truncated_or_unknown_file_is_an_error() {
        let mut file = classic(&[tcp_packet(40000, 502, &MODBUS_REQUEST)]);
        file.truncate(file.len() - 3);
        assert!(payloads(&file, None).is_err());
        assert!(payloads(b"not a capture at all, really", None).is_err());
    }

    #[test]
    fn an_unsupported_link_type_is_an_error() {
        let mut file = classic(&[tcp_packet(40000, 502, b"x")]);
        file[20..24].copy_from_slice(&105u32.to_le_bytes()); // 802.11
        assert!(payloads(&file, None).is_err());
    }
}
