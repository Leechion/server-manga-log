//! NAT traversal (STUN RFC 5389) + UDP relay fallback client (UU feature: wow_detection).
//! Matches `server/relay.js` 11-byte relay packet format:
//!   [4B magic 'LEEC'][1B type][2B channel BE][4B len BE][payload]

use anyhow::{ensure, Context, Result};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::Duration;

const STUN_MAGIC_COOKIE: u32 = 0x2112_A442;
const STUN_BINDING_REQUEST: u16 = 0x0001;
const STUN_BINDING_RESPONSE: u16 = 0x0101;
const ATTR_MAPPED_ADDRESS: u16 = 0x0001;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// Build a 20-byte RFC 5389 STUN Binding Request with the given 96-bit transaction ID.
pub fn build_stun_binding_request(tx_id: [u8; 12]) -> [u8; 20] {
    let mut pkt = [0u8; 20];
    pkt[0..2].copy_from_slice(&STUN_BINDING_REQUEST.to_be_bytes());
    pkt[2..4].copy_from_slice(&0u16.to_be_bytes()); // message length = 0
    pkt[4..8].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    pkt[8..20].copy_from_slice(&tx_id);
    pkt
}

/// Parse a STUN Binding Response, verifying transaction ID and extracting the
/// server-reflexive `SocketAddr` from `XOR-MAPPED-ADDRESS` (preferred) or `MAPPED-ADDRESS`.
pub fn parse_stun_binding_response(buf: &[u8], expected_tx_id: &[u8; 12]) -> Result<SocketAddr> {
    ensure!(buf.len() >= 20, "short STUN header");
    let mtype = u16::from_be_bytes([buf[0], buf[1]]);
    ensure!(mtype == STUN_BINDING_RESPONSE, "not STUN binding response ({mtype:#06x})");
    let mlen = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    let cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    ensure!(cookie == STUN_MAGIC_COOKIE, "bad STUN cookie");
    ensure!(&buf[8..20] == expected_tx_id, "STUN tx_id mismatch");
    ensure!(buf.len() >= 20 + mlen, "truncated STUN payload");

    let mut mapped: Option<SocketAddr> = None;
    let mut off = 20usize;
    let end = 20 + mlen;
    while off + 4 <= end {
        let atype = u16::from_be_bytes([buf[off], buf[off + 1]]);
        let alen = u16::from_be_bytes([buf[off + 2], buf[off + 3]]) as usize;
        off += 4;
        ensure!(off + alen <= end, "STUN attribute overflow");
        let val = &buf[off..off + alen];
        if atype == ATTR_XOR_MAPPED_ADDRESS && alen >= 8 {
            let fam = val[1];
            let xport = u16::from_be_bytes([val[2], val[3]]) ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
            if fam == 0x01 {
                let raw = u32::from_be_bytes([val[4], val[5], val[6], val[7]]) ^ STUN_MAGIC_COOKIE;
                return Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(raw)), xport));
            } else if fam == 0x02 && alen >= 20 {
                let mut ipb = [0u8; 16];
                ipb.copy_from_slice(&val[4..20]);
                for i in 0..4 {
                    ipb[i] ^= buf[4 + i];
                }
                for i in 0..12 {
                    ipb[4 + i] ^= expected_tx_id[i];
                }
                return Ok(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(ipb)), xport));
            }
        } else if atype == ATTR_MAPPED_ADDRESS && alen >= 8 && mapped.is_none() {
            let fam = val[1];
            let port = u16::from_be_bytes([val[2], val[3]]);
            if fam == 0x01 {
                mapped = Some(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(val[4], val[5], val[6], val[7])),
                    port,
                ));
            }
        }
        // attributes are padded to 4-byte boundary
        let padded = (alen + 3) & !3;
        off += padded;
    }
    mapped.context("no MAPPED-ADDRESS or XOR-MAPPED-ADDRESS in STUN response")
}

/// Discover our server-reflexive public address via a STUN server.
pub fn stun_probe(stun_server: SocketAddr, timeout: Duration) -> Result<SocketAddr> {
    let sock = UdpSocket::bind("0.0.0.0:0").context("bind udp for STUN")?;
    sock.set_read_timeout(Some(timeout))?;
    let mut tx_id = [0u8; 12];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    tx_id[..8].copy_from_slice(&(nanos as u64).to_be_bytes());
    tx_id[8..12].copy_from_slice(&std::process::id().to_be_bytes());

    let req = build_stun_binding_request(tx_id);
    sock.send_to(&req, stun_server).context("STUN send_to")?;
    let mut buf = [0u8; 512];
    let (n, _) = sock.recv_from(&mut buf).context("STUN recv_from")?;
    parse_stun_binding_response(&buf[..n], &tx_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatType {
    DirectOrOpen,
    BehindNat,
    Blocked,
}

/// Quick NAT posture detection comparing local LAN address vs STUN mapped address.
pub fn detect_nat_posture(stun_server: SocketAddr) -> (NatType, Option<SocketAddr>) {
    match stun_probe(stun_server, Duration::from_millis(1200)) {
        Ok(mapped) => {
            let local = crate::pair::local_ip();
            if local.map(|ip| IpAddr::V4(ip) == mapped.ip()).unwrap_or(false) {
                (NatType::DirectOrOpen, Some(mapped))
            } else {
                (NatType::BehindNat, Some(mapped))
            }
        }
        Err(_) => (NatType::Blocked, None),
    }
}

// ---------------------------------------------------------------------------
// UDP Relay Client matching `server/relay.js`
// ---------------------------------------------------------------------------

pub const RELAY_MAGIC: &[u8; 4] = b"LEEC";
pub const RELAY_HDR: usize = 11;

/// Deterministically map a signaling channel name (e.g. `"a7f3bc12"`) into a u16 channel ID.
pub fn channel_id(name: &str) -> u16 {
    if let Ok(n) = name.parse::<u16>() {
        return n;
    }
    let mut h: u32 = 0x811c_9dc5;
    for &b in name.as_bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    ((h >> 16) ^ h) as u16
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayPacket {
    pub ptype: u8,
    pub channel: u16,
    pub payload: Vec<u8>,
}

impl RelayPacket {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(RELAY_HDR + self.payload.len());
        out.extend_from_slice(RELAY_MAGIC);
        out.push(self.ptype);
        out.extend_from_slice(&self.channel.to_be_bytes());
        out.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        ensure!(b.len() >= RELAY_HDR, "short relay header");
        ensure!(&b[0..4] == RELAY_MAGIC, "bad relay magic");
        let ptype = b[4];
        let channel = u16::from_be_bytes([b[5], b[6]]);
        let len = u32::from_be_bytes([b[7], b[8], b[9], b[10]]) as usize;
        ensure!(b.len() >= RELAY_HDR + len, "truncated relay payload");
        Ok(Self {
            ptype,
            channel,
            payload: b[RELAY_HDR..RELAY_HDR + len].to_vec(),
        })
    }
}

pub struct RelayClient {
    sock: UdpSocket,
    server: SocketAddr,
    pub channel: u16,
}

impl RelayClient {
    pub fn connect(server: SocketAddr, channel_name: &str) -> Result<Self> {
        let sock = UdpSocket::bind("0.0.0.0:0").context("bind udp relay")?;
        sock.set_read_timeout(Some(Duration::from_millis(500)))?;
        let me = Self {
            sock,
            server,
            channel: channel_id(channel_name),
        };
        me.heartbeat()?;
        Ok(me)
    }

    /// Send control packet (`ptype = 4`) so `server/relay.js` registers our peer address.
    pub fn heartbeat(&self) -> Result<()> {
        let pkt = RelayPacket {
            ptype: 4,
            channel: self.channel,
            payload: Vec::new(),
        };
        self.sock.send_to(&pkt.encode(), self.server)?;
        Ok(())
    }

    pub fn send(&self, ptype: u8, payload: &[u8]) -> Result<()> {
        let pkt = RelayPacket {
            ptype,
            channel: self.channel,
            payload: payload.to_vec(),
        };
        self.sock.send_to(&pkt.encode(), self.server)?;
        Ok(())
    }

    pub fn recv(&self) -> Result<RelayPacket> {
        let mut buf = vec![0u8; 65535];
        let (n, src) = self.sock.recv_from(&mut buf)?;
        ensure!(src == self.server, "unexpected udp source {src}");
        RelayPacket::decode(&buf[..n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stun_xor_mapped_ipv4_roundtrip() {
        let tx_id = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let req = build_stun_binding_request(tx_id);
        assert_eq!(req.len(), 20);
        assert_eq!(&req[4..8], &STUN_MAGIC_COOKIE.to_be_bytes());

        // synthesize a Binding Response with XOR-MAPPED-ADDRESS 203.0.113.5:54321
        let ip = Ipv4Addr::new(203, 0, 113, 5);
        let port = 54321u16;
        let xport = port ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
        let xip = u32::from(ip) ^ STUN_MAGIC_COOKIE;

        let mut resp = Vec::new();
        resp.extend_from_slice(&STUN_BINDING_RESPONSE.to_be_bytes());
        resp.extend_from_slice(&12u16.to_be_bytes()); // 4B attr hdr + 8B val
        resp.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        resp.extend_from_slice(&tx_id);
        resp.extend_from_slice(&ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
        resp.extend_from_slice(&8u16.to_be_bytes());
        resp.push(0);
        resp.push(0x01); // IPv4
        resp.extend_from_slice(&xport.to_be_bytes());
        resp.extend_from_slice(&xip.to_be_bytes());

        let got = parse_stun_binding_response(&resp, &tx_id).unwrap();
        assert_eq!(got, SocketAddr::new(IpAddr::V4(ip), port));
    }

    #[test]
    fn stun_rejects_mismatched_tx_id() {
        let tx_id = [9u8; 12];
        let bad_tx = [8u8; 12];
        let mut resp = build_stun_binding_request(tx_id);
        resp[0..2].copy_from_slice(&STUN_BINDING_RESPONSE.to_be_bytes());
        assert!(parse_stun_binding_response(&resp, &bad_tx).is_err());
    }

    #[test]
    fn relay_packet_wire_matches_relay_js() {
        let pkt = RelayPacket {
            ptype: 1,
            channel: 0x1234,
            payload: b"hello".to_vec(),
        };
        let b = pkt.encode();
        assert_eq!(&b[0..4], b"LEEC");
        assert_eq!(b[4], 1);
        assert_eq!(&b[5..7], &[0x12, 0x34]);
        assert_eq!(&b[7..11], &5u32.to_be_bytes());
        assert_eq!(&b[11..], b"hello");
        assert_eq!(RelayPacket::decode(&b).unwrap(), pkt);
    }

    #[test]
    fn channel_id_deterministic() {
        assert_eq!(channel_id("42"), 42);
        assert_eq!(channel_id("a7f3bc12"), channel_id("a7f3bc12"));
    }
}
