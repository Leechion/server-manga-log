//! LeeConnect core: capability negotiation + frame protocol + control messages.
//! All self-implemented; no UU code is used.

pub mod capabilities;
pub mod capture;
pub mod clipboard;
pub mod decoder;
pub mod encoder;
pub mod file;
pub mod frame;
pub mod input;
pub mod msg;
pub mod portmap;
pub mod transport;

pub use capabilities::Capabilities;
pub use frame::Frame;

/// DXGI allows only one output duplication per output at a time; real-desktop
/// tests that call `DuplicateOutput` must not run concurrently. Shared lock.
#[cfg(test)]
pub static DESKTOP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
