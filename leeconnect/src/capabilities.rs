//! Capability bitmap. Derived from the UU feature list (ideas, not code) as the
//! spec for what LeeConnect must support. Host advertises its caps, controller
//! intersects with its own, and the session runs the agreed set.

use bitflags::bitflags;

bitflags! {
    /// Negotiable feature bits exchanged during session setup.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Capabilities: u64 {
        // ---- video ----
        const QUALITY_NORMAL   = 1 << 0;
        const FPS_90           = 1 << 1;
        const FPS_144          = 1 << 2;
        const SCREEN_SUPER     = 1 << 3;  // high-bitrate tier
        const SCREEN_HDR       = 1 << 4;  // 10-bit / HDR10
        const SCREEN_CHROMA    = 1 << 5;  // 4:4:4 sampling
        const MULTI_SCREEN     = 1 << 6;
        const SCREEN_VIRTUAL   = 1 << 7;  // virtual display (IDD)
        const FULL_SCREEN      = 1 << 8;
        // ---- input ----
        const DEVICE_MOUSE     = 1 << 9;
        const DEVICE_KEYBOARD  = 1 << 10;
        const SIMULATE_MOUSE   = 1 << 11; // relative / game mode
        const DEVICE_GAMEPAD   = 1 << 12; // ViGEmBus virtual pad
        const INPUT_PROTOCOL_V1= 1 << 13;
        // ---- audio ----
        const AUDIO            = 1 << 14; // system loopback
        const MICROPHONE       = 1 << 15;
        // ---- data ----
        const FILE_TRANS       = 1 << 16;
        const CLIPBOARD        = 1 << 17;
        const PORT_MAPPING     = 1 << 18;
        const TERMINAL         = 1 << 19;
        const REMOTE_APPS      = 1 << 20;
        // ---- privacy / system ----
        const PRIVACY_SCREEN   = 1 << 21;
        const SYSTEM_ACTIONS   = 1 << 22; // reboot/shutdown/lock/taskmgr
        const WAKE_ON_LAN      = 1 << 23;
    }
}

impl Capabilities {
    /// Encode to the wire format (u64 LE inside control messages).
    pub fn to_u64(self) -> u64 { self.bits() }

    /// Decode a peer's advertised set, ignoring unknown bits (forward compatible).
    pub fn from_u64(v: u64) -> Self { Self::from_bits_truncate(v) }

    /// Session capability = intersection of both sides.
    pub fn negotiate(host: Capabilities, controller: Capabilities) -> Capabilities {
        host & controller
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiation_intersects() {
        let host = Capabilities::FILE_TRANS | Capabilities::CLIPBOARD | Capabilities::FPS_144;
        let controller = Capabilities::CLIPBOARD | Capabilities::FPS_144;
        let agreed = Capabilities::negotiate(host, controller);
        assert!(agreed.contains(Capabilities::CLIPBOARD));
        assert!(agreed.contains(Capabilities::FPS_144));
        assert!(!agreed.contains(Capabilities::FILE_TRANS));
    }

    #[test]
    fn unknown_bits_are_dropped() {
        // peer sets a bit we don't know (bit 63) -> truncate ignores it
        let weird = 1u64 << 63;
        let caps = Capabilities::from_u64(weird | Capabilities::AUDIO.bits());
        assert_eq!(caps, Capabilities::AUDIO);
    }
}
