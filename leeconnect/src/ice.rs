//! NAT traversal (STUN RFC 5389) + UDP relay fallback client (UU feature: wow_detection).
//! Matches `server/relay.js` 11-byte relay packet format:
//!   [4B magic 'LEEC'][1B type][2B channel BE][4B len BE][payload]

use anyhow::{ensure, Context, Result};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_STUN_SERVER: &str = "stun.miwifi.com:3478";

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

/// Resolve a `host:port` STUN address to an IPv4 `SocketAddr`.
pub fn resolve_stun(addr: &str) -> Option<SocketAddr> {
    addr.to_socket_addrs()
        .ok()?
        .find(|a| a.is_ipv4())
}

/// Discover our server-reflexive public address using an existing `UdpSocket`
/// (e.g. the same socket that will later be handed to `quinn::Endpoint`).
pub fn stun_probe_on_socket(
    sock: &UdpSocket,
    stun_server: SocketAddr,
    timeout: Duration,
) -> Result<SocketAddr> {
    let prev_timeout = sock.read_timeout().ok().flatten();
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
    let deadline = Instant::now() + timeout;
    let res = loop {
        let rem = deadline.saturating_duration_since(Instant::now());
        if rem.is_zero() {
            break Err(anyhow::anyhow!("STUN recv_from timeout"));
        }
        let _ = sock.set_read_timeout(Some(rem));
        match sock.recv_from(&mut buf) {
            Ok((n, _)) => {
                if let Ok(mapped) = parse_stun_binding_response(&buf[..n], &tx_id) {
                    break Ok(mapped);
                }
            }
            Err(e) => break Err(anyhow::anyhow!("STUN recv_from: {e}")),
        }
    };
    let _ = sock.set_read_timeout(prev_timeout);
    res
}

/// Discover our server-reflexive public address via a STUN server.
pub fn stun_probe(stun_server: SocketAddr, timeout: Duration) -> Result<SocketAddr> {
    let sock = UdpSocket::bind("0.0.0.0:0").context("bind udp for STUN")?;
    stun_probe_on_socket(&sock, stun_server, timeout)
}

/// Send small non-QUIC UDP punch packets to open a NAT pinhole toward `peer`.
pub fn send_udp_punch(sock: &UdpSocket, peer: SocketAddr, count: usize) {
    for _ in 0..count {
        let _ = sock.send_to(b"LEECPUNCH", peer);
    }
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

/// Transparent localhost UDP <-> `server/relay.js` bridge so QUIC (`quinn`) can
/// run unmodified over the UDP relay when direct P2P hole-punching fails.
pub struct RelayUdpBridge {
    local_addr: SocketAddr,
    stop: Arc<AtomicBool>,
}

impl RelayUdpBridge {
    /// Controller side: bind a local UDP port (`local_addr()`); `quinn` connects to it.
    pub fn connect_client(relay_server: SocketAddr, channel_name: &str) -> Result<Self> {
        Self::spawn_bridge(relay_server, channel_name, None)
    }

    /// Host side: bridge packets from `relay_server` directly to `target_local_quic` (e.g. `127.0.0.1:44330`).
    pub fn connect_host(
        relay_server: SocketAddr,
        channel_name: &str,
        target_local_quic: SocketAddr,
    ) -> Result<Self> {
        Self::spawn_bridge(relay_server, channel_name, Some(target_local_quic))
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    fn spawn_bridge(
        relay_server: SocketAddr,
        channel_name: &str,
        initial_peer: Option<SocketAddr>,
    ) -> Result<Self> {
        let local_sock = UdpSocket::bind("127.0.0.1:0").context("bind local relay bridge")?;
        let local_addr = local_sock.local_addr()?;
        local_sock.set_read_timeout(Some(Duration::from_millis(200)))?;

        let relay_sock = UdpSocket::bind("0.0.0.0:0").context("bind external relay socket")?;
        relay_sock.set_read_timeout(Some(Duration::from_millis(200)))?;

        let channel = channel_id(channel_name);
        let hb_pkt = RelayPacket {
            ptype: 4,
            channel,
            payload: Vec::new(),
        }
        .encode();
        relay_sock.send_to(&hb_pkt, relay_server)?;

        let stop = Arc::new(AtomicBool::new(false));
        let peer_slot = Arc::new(Mutex::new(initial_peer));

        let local_sock_in = local_sock.try_clone()?;
        let relay_sock_out = relay_sock.try_clone()?;
        let stop_out = stop.clone();
        let peer_out = peer_slot.clone();
        std::thread::Builder::new()
            .name("relay-out".into())
            .spawn(move || {
                let mut buf = vec![0u8; 65535];
                let mut last_hb = Instant::now();
                while !stop_out.load(Ordering::Relaxed) {
                    if last_hb.elapsed() >= Duration::from_secs(15) {
                        let _ = relay_sock_out.send_to(&hb_pkt, relay_server);
                        last_hb = Instant::now();
                    }
                    match local_sock.recv_from(&mut buf) {
                        Ok((n, src)) => {
                            let mut guard = peer_out.lock().unwrap();
                            if guard.is_none() {
                                *guard = Some(src);
                            }
                            drop(guard);
                            let pkt = RelayPacket {
                                ptype: 0,
                                channel,
                                payload: buf[..n].to_vec(),
                            };
                            let _ = relay_sock_out.send_to(&pkt.encode(), relay_server);
                        }
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => break,
                    }
                }
            })?;

        let stop_in = stop.clone();
        std::thread::Builder::new()
            .name("relay-in".into())
            .spawn(move || {
                let mut buf = vec![0u8; 65535];
                while !stop_in.load(Ordering::Relaxed) {
                    match relay_sock.recv_from(&mut buf) {
                        Ok((n, src)) if src == relay_server => {
                            if let Ok(pkt) = RelayPacket::decode(&buf[..n]) {
                                if pkt.ptype == 0 && pkt.channel == channel {
                                    if let Some(target) = *peer_slot.lock().unwrap() {
                                        let _ = local_sock_in.send_to(&pkt.payload, target);
                                    }
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => break,
                    }
                }
            })?;

        Ok(Self { local_addr, stop })
    }
}

impl Drop for RelayUdpBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
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

    #[test]
    fn relay_bridge_forwards_both_directions() {
        // Mock relay.js server on localhost
        let relay = UdpSocket::bind("127.0.0.1:0").unwrap();
        let relay_addr = relay.local_addr().unwrap();
        relay.set_read_timeout(Some(Duration::from_secs(2))).unwrap();

        let host_quic = UdpSocket::bind("127.0.0.1:0").unwrap();
        let host_quic_addr = host_quic.local_addr().unwrap();
        host_quic.set_read_timeout(Some(Duration::from_secs(2))).unwrap();

        let ctrl_quic = UdpSocket::bind("127.0.0.1:0").unwrap();
        ctrl_quic.set_read_timeout(Some(Duration::from_secs(2))).unwrap();

        let _host_bridge = RelayUdpBridge::connect_host(relay_addr, "chan1", host_quic_addr).unwrap();
        let ctrl_bridge = RelayUdpBridge::connect_client(relay_addr, "chan1").unwrap();

        // Relay thread: pair the two bridge endpoints and forward non-control packets
        let relay_handle = std::thread::spawn(move || {
            let mut peers: Vec<SocketAddr> = Vec::new();
            let mut buf = [0u8; 2048];
            for _ in 0..8 {
                let Ok((n, src)) = relay.recv_from(&mut buf) else { break };
                let Ok(pkt) = RelayPacket::decode(&buf[..n]) else { continue };
                if pkt.ptype == 4 {
                    if !peers.contains(&src) {
                        peers.push(src);
                    }
                } else {
                    for &p in &peers {
                        if p != src {
                            let _ = relay.send_to(&buf[..n], p);
                        }
                    }
                }
            }
        });

        // Give heartbeats a moment to register both peers
        std::thread::sleep(Duration::from_millis(60));
        ctrl_quic.send_to(b"ping", ctrl_bridge.local_addr()).unwrap();

        let mut buf = [0u8; 64];
        let (n, from_host_bridge) = host_quic.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"ping");

        host_quic.send_to(b"pong", from_host_bridge).unwrap();
        let (n2, _) = ctrl_quic.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n2], b"pong");

        drop(ctrl_bridge);
        drop(_host_bridge);
        let _ = relay_handle.join();
    }
}
