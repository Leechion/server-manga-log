//! M2 end-to-end loopback demo: capture -> RawEncoder -> QUIC -> decode -> save.
//! Run: cargo run --bin loopback -- [frames] [out_dir]
//! Saves the first frame as .ppm (viewable) and reports fps.

use anyhow::Result;
use leeconnect::capture::DesktopDup;
use leeconnect::encoder::{EncodedFrame, PixelFormat, RawEncoder, VideoEncoder};
use leeconnect::frame::{Frame, FrameType};
use leeconnect::transport;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[tokio::main]
async fn main() -> Result<()> {
    let nframes: u32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let out_dir = std::env::args().nth(2).unwrap_or_else(|| "target/loopback".into());
    std::fs::create_dir_all(&out_dir).ok();

    let (sc, ca_der) = transport::gen_server_config()?;
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let mut server = transport::bind_server(addr, sc)?;
    let bound = server.local_addr()?;

    // receiver task
    let out2 = out_dir.clone();
    let receiver = tokio::spawn(async move {
        let conn = server.accept().await.unwrap().await.unwrap();
        let (_send, mut recv) = conn.accept_bi().await.unwrap();
        let mut got = 0u32;
        let mut bytes = 0usize;
        let t0 = std::time::Instant::now();
        let mut first: Option<EncodedFrame> = None;
        while let Ok(f) = transport::read_frame(&mut recv).await {
            if f.ftype != FrameType::Video {
                break;
            }
            bytes += f.payload.len();
            let ef = EncodedFrame::from_wire(&f.payload).expect("decode video");
            if got == 0 {
                first = Some(ef);
            }
            got += 1;
            if got >= nframes {
                break;
            }
        }
        let dt = t0.elapsed();
        if let Some(ef) = first {
            let path = format!("{out2}/frame0.ppm");
            if ef.format == PixelFormat::Bgra8 {
                save_ppm(&path, &ef).expect("save ppm");
                // mean luma diagnostic: is the frame actually black?
                let mut sum: u64 = 0;
                let mut nonzero = 0u64;
                for px in ef.data.chunks_exact(4) {
                    let l = (px[0] as u64 + px[1] as u64 + px[2] as u64) / 3;
                    sum += l;
                    if l > 4 {
                        nonzero += 1;
                    }
                }
                let n = (ef.data.len() / 4).max(1) as f64;
                println!(
                    "saved {path} ({}x{}) mean-luma {:.1} nonblack-pixels {:.1}%",
                    ef.width,
                    ef.height,
                    sum as f64 / n,
                    100.0 * nonzero as f64 / n
                );
            }
        }
        println!(
            "received {got} frames, {:.1} MB, {:.1} fps (loopback)",
            bytes as f64 / 1e6,
            got as f64 / dt.as_secs_f64().max(0.001)
        );
        got
    });

    // sender: capture + encode + send
    let client = transport::client_endpoint(&ca_der)?;
    let conn = client.connect(bound, transport::SNI)?.await?;
    let (mut send, _recv) = conn.open_bi().await?;

    let mut dup = DesktopDup::new_primary()?;
    let (w, h) = dup.size();
    println!("capturing {w}x{h} via DXGI, {nframes} frames");
    let mut enc = RawEncoder::new();
    let mut seq = 0u32;
    let t0 = std::time::Instant::now();
    while seq < nframes {
        // acquire with timeout; if static desktop returns None, force keyframe path by sleeping
        match dup.acquire(33)? {
            Some(f) => {
                let ef = enc.encode(&f, seq as u64)?;
                transport::write_frame(&mut send, &Frame::new(FrameType::Video, 0, seq, ef.to_wire())).await?;
                seq += 1;
            }
            None => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
    let dt = t0.elapsed();
    println!("sent {seq} frames in {:.2}s ({:.1} fps capture+encode)", dt.as_secs_f64(), seq as f64 / dt.as_secs_f64());
    send.finish()?;

    let got = receiver.await?;
    conn.close(0u32.into(), b"bye");
    assert_eq!(got, nframes, "all frames received");
    println!("M2 loopback OK: capture->encode->QUIC->decode->save verified");
    Ok(())
}

/// Save Bgra8 frame as binary PPM (RGB).
fn save_ppm(path: &str, ef: &EncodedFrame) -> Result<()> {
    let w = ef.width as usize;
    let h = ef.height as usize;
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for px in ef.data.chunks_exact(4) {
        out.push(px[2]); // R
        out.push(px[1]); // G
        out.push(px[0]); // B
    }
    std::fs::write(path, out)?;
    Ok(())
}
