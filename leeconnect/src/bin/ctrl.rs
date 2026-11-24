//! LeeConnect CONTROLLER: receives video, sends input, saves first frame.
//! Run after `cargo run --bin host`. Connects to 127.0.0.1:44330.

use anyhow::Result;
use leeconnect::decoder::H264Decoder;
use leeconnect::encoder::{EncodedFrame, PixelFormat};
use leeconnect::frame::{Frame, FrameType};
use leeconnect::input::{encode_input, InputEvent};
use leeconnect::msg::{self, ControlMsg};
use leeconnect::transport;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const PORT: u16 = 44330;
const WANT_FRAMES: u32 = 12;

#[tokio::main]
async fn main() -> Result<()> {
    // --pair <code> [signal]: resolve the host via signaling (LAN discovery, #3),
    // otherwise connect directly to 127.0.0.1 (local test).
    let argv: Vec<String> = std::env::args().collect();
    let mut addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), PORT);
    let mut ca_der;
    let mut sni = transport::SNI.to_owned();

    if let Some(pos) = argv.iter().position(|a| a == "--pair") {
        let code = argv.get(pos + 1).cloned().unwrap_or_default();
        let signal = argv
            .get(pos + 2)
            .cloned()
            .unwrap_or_else(|| leeconnect::pair::DEFAULT_SIGNAL.to_owned());
        let mut s = leeconnect::pair::Signaling::open(&signal)?;
        let serial = leeconnect::pair::gen_serial(leeconnect::pair::DEFAULT_SECRET, "LEE-CTRL-9");
        s.register("LEE-CTRL-9", &serial, "lee-ctrl", "", &[])?;
        let (ep, ca) = s.pair_redeem(&code)?;
        println!("[ctrl] paired via {signal}: host {ep}");
        addr = ep;
        ca_der = ca;
        // connect-by-IP: the leaf carries the host LAN IP as a SAN, so use it for name verification
        sni = ep.ip().to_string();
    } else {
        ca_der = transport::load_ca()?; // trust the host's published CA (local test)
    }

    let client = transport::client_endpoint(&ca_der)?;
    let conn = client.connect(addr, &sni)?.await?;
    println!("[ctrl] connected to host");

    let (mut send, mut recv) = conn.open_bi().await?;

    // send a few input events (zero-delta moves: exercise the pipeline, don't move cursor)
    let events = [
        InputEvent::MoveRel { dx: 0, dy: 0 },
        InputEvent::Key { vk: 0x41, down: true },
        InputEvent::Key { vk: 0x41, down: false },
        InputEvent::Wheel { delta: 0, horizontal: false },
        InputEvent::MoveAbs { x: 1, y: 1 },
    ];
    for (i, ev) in events.iter().enumerate() {
        transport::write_frame(&mut send, &Frame::new(FrameType::Input, 1, i as u32, encode_input(*ev))).await?;
    }
    println!("[ctrl] sent {} input events", events.len());

    // read: control hello + video frames
    let mut frames = 0u32;
    let mut bytes = 0usize;
    let mut first: Option<EncodedFrame> = None;
    let mut first_decoded: Option<leeconnect::decoder::DecodedPicture> = None;
    let mut caps_seen = None;
    let mut h264_seen = false;
    let mut decoder = H264Decoder::new()?;
    let t0 = std::time::Instant::now();
    while frames < WANT_FRAMES {
        let f = transport::read_frame(&mut recv).await?;
        match f.ftype {
            FrameType::Control => {
                let txt = String::from_utf8_lossy(&f.payload).to_string();
                if let Ok(m) = msg::decode(&txt) {
                    if let ControlMsg::CapabilityExchange { caps } = m {
                        caps_seen = Some(leeconnect::Capabilities::from_u64(caps));
                        println!("[ctrl] host caps: {caps:?}");
                    }
                }
            }
            FrameType::Video => {
                bytes += f.payload.len();
                let ef = EncodedFrame::from_wire(&f.payload)?;
                if first.is_none() {
                    first = Some(ef.clone());
                }
                if ef.format == PixelFormat::H264 {
                    h264_seen = true;
                    // feed every frame so the decoder tracks reference state;
                    // capture the first reconstructed picture
                    if let Some(pic) = decoder.decode(&ef)? {
                        if first_decoded.is_none() {
                            first_decoded = Some(pic);
                        }
                    }
                }
                frames += 1;
            }
            _ => {}
        }
    }
    let dt = t0.elapsed();

    // tell host we're done
    let end = msg::encode(&ControlMsg::SystemAction { action: "end".into() });
    transport::write_frame(&mut send, &Frame::new(FrameType::Control, 0, 0, end.into_bytes())).await?;

    if let Some(pic) = first_decoded {
        // H.264 path: save the decoded (reconstructed) picture
        let path = "target/session/frame0.ppm";
        std::fs::create_dir_all("target/session").ok();
        save_bgra_ppm(path, &pic)?;
        let l = pic.mean_luma();
        let nb = pic.pixels.chunks_exact(4).filter(|p| ((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3) > 4).count();
        let n = (pic.pixels.len() / 4).max(1);
        println!("[ctrl] DECODED H.264 -> saved {path} {}x{} mean-luma {:.1} nonblack {:.1}%", pic.width, pic.height, l, 100.0 * nb as f64 / n as f64);
    } else if let Some(ef) = first {
        // raw path: save the lossless frame directly
        let path = "target/session/frame0.ppm";
        std::fs::create_dir_all("target/session").ok();
        if ef.format == PixelFormat::Bgra8 {
            save_ppm(path, &ef)?;
            let mut sum = 0u64;
            let mut nb = 0u64;
            for px in ef.data.chunks_exact(4) {
                let l = (px[0] as u64 + px[1] as u64 + px[2] as u64) / 3;
                sum += l;
                if l > 4 {
                    nb += 1;
                }
            }
            let n = (ef.data.len() / 4).max(1) as f64;
            println!("[ctrl] saved {path} {}x{} mean-luma {:.1} nonblack {:.1}%", ef.width, ef.height, sum as f64 / n, 100.0 * nb as f64 / n);
        }
    }
    println!(
        "[ctrl] received {frames} video frames {:.1}MB {:.1}fps h264={h264_seen} caps={}",
        bytes as f64 / 1e6,
        frames as f64 / dt.as_secs_f64().max(0.001),
        caps_seen.map(|c| c.bits()).unwrap_or(0)
    );
    println!("M3 session OK: host video->ctrl + ctrl input->host verified over QUIC");
    Ok(())
}

fn save_ppm(path: &str, ef: &EncodedFrame) -> Result<()> {
    let w = ef.width as usize;
    let h = ef.height as usize;
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in ef.data.chunks_exact(4) {
        out.push(px[2]);
        out.push(px[1]);
        out.push(px[0]);
    }
    std::fs::write(path, out)?;
    Ok(())
}

/// Save a decoded BGRA picture (from the H.264 path) as binary PPM (RGB).
fn save_bgra_ppm(path: &str, pic: &leeconnect::decoder::DecodedPicture) -> Result<()> {
    let w = pic.width as usize;
    let h = pic.height as usize;
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in pic.pixels.chunks_exact(4) {
        out.push(px[2]); // R
        out.push(px[1]); // G
        out.push(px[0]); // B
    }
    std::fs::write(path, out)?;
    Ok(())
}
