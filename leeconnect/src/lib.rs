//! LeeConnect core: capability negotiation + frame protocol + control messages.
//! All self-implemented; no UU code is used.

pub mod annotation;
pub mod apps;
pub mod audio;
pub mod capabilities;
pub mod capture;
pub mod clipboard;
pub mod cursor;
pub mod decoder;
pub mod display;
pub mod encoder;
pub mod file;
pub mod frame;
pub mod gamepad;
pub mod ice;
pub mod input;
pub mod msg;
pub mod mumu;
pub mod pair;
pub mod portmap;
pub mod privacy;
pub mod stats;
pub mod system;
pub mod transport;
pub mod view;

pub use capabilities::Capabilities;
pub use frame::Frame;

/// DXGI allows only one output duplication per output at a time; real-desktop
/// tests that call `DuplicateOutput` must not run concurrently. Shared lock.
#[cfg(test)]
pub static DESKTOP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
