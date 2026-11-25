//! Pairing & signaling client (UU feature: remote_connection_cli / pairing code).
//! Talks to `server/signaling.js` over WebSocket (tungstenite, blocking) and
//! generates the local SERIAL that the server's auth.js verifies — byte-identical
//! HMAC-SHA256 + RFC4648 base32 grouping, proven by cross-check unit tests.
//!
//! Flow (LAN): host `register(endpoint)` -> `pair_create` -> shows 6-digit code.
//!             ctrl `register` -> `pair_redeem(code)` -> gets host QUIC endpoint
//!             -> connects QUIC directly.

use anyhow::{bail, Context, Result};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::net::{IpAddr, SocketAddr};
use tungstenite::{connect, Message, WebSocket};

pub const DEFAULT_SECRET: &str = "[SECRET]";
pub const DEFAULT_SIGNAL: &str = "127.0.0.1:9700";

type HmacSha256 = Hmac<Sha256>;

/// RFC4648 base32 (no padding), matching Node auth.js base32().
pub fn base32(data: &[u8]) -> String {
    const A: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let mut bits = 0u32;
    let mut value = 0u32;
    for &b in data {
        value = (value << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(A[((value >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(A[((value << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// SERIAL = grouped base32(HMAC-SHA256(secret, "machine|full"))[0..20].
pub fn gen_serial(secret: &str, machine: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(format!("{machine}|full").as_bytes());
    let h = mac.finalize().into_bytes();
    let s = base32(&h);
    s[..20]
        .as_bytes()
        .chunks(5)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("-")
}

/// Constant-shape verify (regenerate + string compare; fixture-grade secret).
pub fn verify_serial(secret: &str, machine: &str, serial: &str) -> bool {
    gen_serial(secret, machine) == serial
}

/// Best-guess LAN IPv4 of this machine (UDP route trick, no packets sent).
pub fn local_ip() -> Option<std::net::Ipv4Addr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("223.5.5.5:53").ok()?;
    match s.local_addr().ok()?.ip() {
        IpAddr::V4(v) => Some(v),
        _ => None,
    }
}

pub fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn from_hex(s: &str) -> Result<Vec<u8>> {
    anyhow::ensure!(s.len() % 2 == 0, "odd hex length");
    let mut out = Vec::with_capacity(s.len() / 2);
    for i in (0..s.len()).step_by(2) {
        out.push(u8::from_str_radix(&s[i..i + 2], 16).context("hex digit")?);
    }
    Ok(out)
}

/// Thin blocking WS session against signaling.js.
pub struct Signaling {
    ws: WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    pub device_id: String,
}

impl Signaling {
    pub fn open(signal: &str) -> Result<Self> {
        let url = format!("ws://{signal}");
        let (mut ws, _) = connect(&url).with_context(|| format!("ws connect {url}"))?;
        // 5s socket read budget so a silent server can't hang the caller
        use tungstenite::stream::MaybeTlsStream;
        if let MaybeTlsStream::Plain(s) = ws.get_mut() {
            let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
        }
        Ok(Self { ws, device_id: String::new() })
    }

    fn send_json(&mut self, v: &Value) -> Result<()> {
        self.ws
            .send(Message::Text(v.to_string().into()))
            .context("ws send")
    }

    /// Read the server's terminal reply for the last request. Signaling.js answers
    /// every command with exactly one message, so we classify by its `type`:
    /// success types pass through, known failure types become an Err.
    fn recv_reply(&mut self, want: &str) -> Result<Value> {
        for _ in 0..8 {
            let msg = self.ws.read().context("ws read (timeout or closed)")?;
            let Message::Text(t) = msg else { continue };
            let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
            let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
            if ty == want {
                return Ok(v);
            }
            // terminal failures
            if matches!(ty, "auth_fail" | "pair_fail" | "error") {
                bail!("signaling rejected ({ty}): {v}")
            }
            // otherwise (e.g. heartbeat_ack from a prior tick) keep reading
        }
        bail!("no {want} reply")
    }

    /// Register this device with its QUIC endpoint + hex-encoded CA DER (host),
    /// or ("", "") for a controller.
    pub fn register(&mut self, device_id: &str, serial: &str, name: &str, endpoint: &str, ca_der: &[u8]) -> Result<()> {
        self.device_id = device_id.to_owned();
        self.send_json(&serde_json::json!({
            "type": "register", "device_id": device_id, "serial": serial,
            "name": name, "endpoint": endpoint, "ca": to_hex(ca_der)
        }))?;
        self.recv_reply("registered")?;
        Ok(())
    }

    /// Host: mint a 6-digit pairing code (5 min TTL server-side).
    pub fn pair_create(&mut self) -> Result<String> {
        self.send_json(&serde_json::json!({ "type": "pair_create" }))?;
        let v = self.recv_reply("pair_code")?;
        v.get("code")
            .and_then(|c| c.as_str())
            .map(|s| s.to_owned())
            .context("pair_code missing")
    }

    /// Ctrl: redeem code -> host QUIC endpoint + pinned CA DER from the host's
    /// registration (so cross-machine works without shipping the file out-of-band).
    pub fn pair_redeem(&mut self, code: &str) -> Result<(SocketAddr, Vec<u8>)> {
        self.send_json(&serde_json::json!({
            "type": "pair_redeem", "code": code, "controller_id": self.device_id
        }))?;
        let v = self.recv_reply("pair_ok")?;
        let ep = v
            .get("endpoint")
            .and_then(|e| e.as_str())
            .with_context(|| format!("pair_ok missing endpoint: {v}"))?;
        let addr = ep.parse().with_context(|| format!("bad endpoint {ep:?}"))?;
        let ca_hex = v
            .get("ca")
            .and_then(|c| c.as_str())
            .with_context(|| format!("pair_ok missing ca: {v}"))?;
        Ok((addr, from_hex(ca_hex)?))
    }

    pub fn heartbeat(&mut self) -> Result<()> {
        self.send_json(&serde_json::json!({ "type": "heartbeat" }))
    }

    /// Forward an ICE candidate blob to the paired peer via signaling.
    pub fn send_ice(&mut self, candidate: &str) -> Result<()> {
        self.send_json(&serde_json::json!({ "type": "ice", "candidate": candidate }))
    }

    /// Request a UDP relay assignment from signaling.js when direct NAT traversal fails.
    pub fn relay_assign(&mut self) -> Result<(SocketAddr, String)> {
        self.send_json(&serde_json::json!({ "type": "relay_assign" }))?;
        let v = self.recv_reply("relay_assign")?;
        let host = v
            .get("host")
            .and_then(|x| x.as_str())
            .context("relay_assign missing host")?;
        let port = v
            .get("port")
            .and_then(|x| x.as_u64())
            .context("relay_assign missing port")? as u16;
        let channel = v
            .get("channel")
            .and_then(|x| x.as_str())
            .context("relay_assign missing channel")?
            .to_owned();
        let addr: SocketAddr = format!("{host}:{port}").parse().context("bad relay addr")?;
        Ok((addr, channel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_matches_node_keygen() {
        // reference vectors produced by `node tools/keygen.js gen --machine ...`
        assert_eq!(gen_serial(DEFAULT_SECRET, "LEE-HOST-1"), "UHZSM-ESI4V-KINBP-L33M6");
        assert_eq!(gen_serial(DEFAULT_SECRET, "LEE-CTRL-9"), "5HSHG-4TG2A-NBUHU-MWS3T");
        assert!(verify_serial(DEFAULT_SECRET, "LEE-HOST-1", "UHZSM-ESI4V-KINBP-L33M6"));
        assert!(!verify_serial(DEFAULT_SECRET, "LEE-HOST-1", "UHZSM-ESI4V-KINBP-L33M7"));
    }

    #[test]
    fn base32_rfc4648_vectors() {
        assert_eq!(base32(b"foo"), "MZXW6");
        assert_eq!(base32(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32(&[0u8; 32]), "A".repeat(52)); // 256 bits -> ceil(256/5)=52 chars
    }

    #[test]
    fn serial_shape() {
        let s = gen_serial(DEFAULT_SECRET, "ANY");
        // 20 base32 chars in 4 groups of 5, trailing dash dropped => 23 chars, 3 dashes
        assert_eq!(s.len(), 23);
        assert_eq!(s.chars().filter(|&c| c == '-').count(), 3);
    }
}
