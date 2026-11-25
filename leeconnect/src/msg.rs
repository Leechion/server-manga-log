//! Control-plane messages. JSON over the signaling WebSocket (matches the Node
//! signaling server's `type` field). Data-plane frames live in `frame.rs`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorDesc {
    pub index: usize,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppEntry {
    pub name: String,
    pub path: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMsg {
    /// device comes online and advertises its capability set
    Hello { device_id: String, name: String, caps: u64 },
    /// controlled side requests a pairing code
    PairCode { code: String },
    /// controller redeems a pairing code
    PairRedeem { code: String, controller_id: String },
    /// both sides exchange the agreed capability set
    CapabilityExchange { caps: u64 },
    /// transport negotiation (opaque SDP-style blobs)
    Offer { sdp: String },
    Answer { sdp: String },
    Ice { candidate: String },
    Heartbeat,
    /// NAT failure -> fall back to relay
    RelayAssign { host: String, port: u16, channel: String },
    /// one-shot system action RPC
    SystemAction { action: String },
    /// host's result of a system action, echoed back to the controller
    SystemActionResult { action: String, ok: bool, detail: String },
    /// multi-monitor list & runtime switching
    MonitorList { monitors: Vec<MonitorDesc>, active: usize },
    SwitchMonitor { index: usize },
    /// dynamic encoder quality/chroma/HDR tier
    QualityConfig { bitrate_bps: u32, fps: u8, chroma_444: bool, hdr: bool },
    /// follow-mode resolution sync
    SetResolution { width: u32, height: u32 },
    RestoreResolution,
    /// remote application list & quick launch
    AppListRequest,
    AppList { apps: Vec<AppEntry> },
    LaunchApp { path: String },
    LaunchAppResult { name: String, ok: bool, detail: String },
    /// screen annotation strokes (normalized 0..65535 coords)
    AnnotationStroke { color: [u8; 4], width_px: u8, points: Vec<[u16; 2]> },
    AnnotationClear,
    /// enable/disable host audio streaming
    AudioToggle { enabled: bool },
}

pub fn encode(m: &ControlMsg) -> String {
    serde_json::to_string(m).expect("ControlMsg always serializes")
}

pub fn decode(s: &str) -> Result<ControlMsg, serde_json::Error> {
    serde_json::from_str(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::Capabilities;

    #[test]
    fn hello_roundtrip_with_caps() {
        let caps = (Capabilities::AUDIO | Capabilities::CLIPBOARD).to_u64();
        let m = ControlMsg::Hello { device_id: "dev-1".into(), name: "host".into(), caps };
        let s = encode(&m);
        let d = decode(&s).unwrap();
        assert_eq!(m, d);
        // wire uses the snake_case `type` tag the Node server keys on
        assert!(s.contains("\"type\":\"hello\""));
    }

    #[test]
    fn caps_survive_roundtrip() {
        let caps = Capabilities::from_u64((Capabilities::FILE_TRANS | Capabilities::SCREEN_HDR).to_u64());
        assert!(caps.contains(Capabilities::FILE_TRANS));
        assert!(caps.contains(Capabilities::SCREEN_HDR));
    }

    #[test]
    fn system_action_roundtrip() {
        let m = ControlMsg::SystemAction { action: "reboot:confirm".into() };
        let d = decode(&encode(&m)).unwrap();
        assert_eq!(m, d);

        let r = ControlMsg::SystemActionResult {
            action: "lock".into(),
            ok: true,
            detail: "workstation locked".into(),
        };
        let s = encode(&r);
        assert!(s.contains("\"type\":\"system_action_result\""));
        assert_eq!(decode(&s).unwrap(), r);
    }
}
