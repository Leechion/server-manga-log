//! File transfer over the QUIC data plane (UU feature: file_trans_v1..v4,
//! manual_transfer). Chunked protocol on FrameType::File frames.
//!
//! payload layout: [1B op][8B id][8B offset][4B dlen][data]
//!   op 0 = Meta   data: [4B name-len][name utf8][8B size]
//!   op 1 = Chunk  data: raw bytes at offset
//!   op 2 = Done   data: empty
//!   op 3 = Req    data: [4B path-len][path utf8]  (controller -> host)

use anyhow::{bail, ensure, Context, Result};
use crate::frame::{Frame, FrameType};
use crate::transport;
use quinn::{RecvStream, SendStream};
use std::path::{Path, PathBuf};

pub const CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOp {
    Meta { id: u64, name: String, size: u64 },
    Chunk { id: u64, offset: u64, data: Vec<u8> },
    Done { id: u64 },
    Req { id: u64, path: String },
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}
fn be64(b: &[u8]) -> u64 {
    u64::from_be_bytes(b[..8].try_into().unwrap())
}

impl FileOp {
    pub fn to_payload(&self) -> Vec<u8> {
        let mut b = Vec::new();
        match self {
            FileOp::Meta { id, name, size } => {
                b.push(0);
                b.extend_from_slice(&id.to_be_bytes());
                b.extend_from_slice(&0u64.to_be_bytes());
                let nb = name.as_bytes();
                let data = build(|d| {
                    d.extend_from_slice(&(nb.len() as u32).to_be_bytes());
                    d.extend_from_slice(nb);
                    d.extend_from_slice(&size.to_be_bytes());
                });
                b.extend_from_slice(&(data.len() as u32).to_be_bytes());
                b.extend_from_slice(&data);
            }
            FileOp::Chunk { id, offset, data } => {
                b.push(1);
                b.extend_from_slice(&id.to_be_bytes());
                b.extend_from_slice(&offset.to_be_bytes());
                b.extend_from_slice(&(data.len() as u32).to_be_bytes());
                b.extend_from_slice(data);
            }
            FileOp::Done { id } => {
                b.push(2);
                b.extend_from_slice(&id.to_be_bytes());
                b.extend_from_slice(&0u64.to_be_bytes());
                b.extend_from_slice(&0u32.to_be_bytes());
            }
            FileOp::Req { id, path } => {
                b.push(3);
                b.extend_from_slice(&id.to_be_bytes());
                b.extend_from_slice(&0u64.to_be_bytes());
                let data = build(|d| {
                    let pb = path.as_bytes();
                    d.extend_from_slice(&(pb.len() as u32).to_be_bytes());
                    d.extend_from_slice(pb);
                });
                b.extend_from_slice(&(data.len() as u32).to_be_bytes());
                b.extend_from_slice(&data);
            }
        }
        b
    }

    pub fn from_payload(b: &[u8]) -> Result<Self> {
        ensure!(b.len() >= 21, "short file payload");
        let op = b[0];
        let id = be64(&b[1..9]);
        let offset = be64(&b[9..17]);
        let dlen = be32(&b[17..21]) as usize;
        ensure!(b.len() >= 21 + dlen, "truncated file data");
        let d = &b[21..21 + dlen];
        Ok(match op {
            0 => {
                ensure!(d.len() >= 12, "short meta");
                let nl = be32(d) as usize;
                ensure!(d.len() >= 4 + nl + 8, "meta name overflow");
                let name = String::from_utf8(d[4..4 + nl].to_vec()).context("meta name utf8")?;
                let size = be64(&d[4 + nl..]);
                FileOp::Meta { id, name, size }
            }
            1 => FileOp::Chunk { id, offset, data: d.to_vec() },
            2 => FileOp::Done { id },
            3 => {
                ensure!(d.len() >= 4, "short req");
                let pl = be32(d) as usize;
                ensure!(d.len() >= 4 + pl, "req path overflow");
                let path = String::from_utf8(d[4..4 + pl].to_vec()).context("req path utf8")?;
                FileOp::Req { id, path }
            }
            o => bail!("unknown file op {o}"),
        })
    }

    pub fn into_frame(&self, seq: u32) -> Frame {
        Frame::new(FrameType::File, 5, seq, self.to_payload())
    }
}

fn build(f: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut v = Vec::new();
    f(&mut v);
    v
}

/// Streams a local file to the peer as Meta + Chunks + Done.
pub async fn send_file(path: &Path, send: &mut SendStream) -> Result<u64> {
    let data = tokio::fs::read(path).await.with_context(|| format!("read {path:?}"))?;
    let id = 1u64;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    transport::write_frame(send, &FileOp::Meta { id, name, size: data.len() as u64 }.into_frame(0)).await?;
    let mut off = 0usize;
    let mut seq = 1u32;
    while off < data.len() {
        let end = (off + CHUNK).min(data.len());
        transport::write_frame(send, &FileOp::Chunk { id, offset: off as u64, data: data[off..end].to_vec() }.into_frame(seq)).await?;
        off = end;
        seq += 1;
    }
    transport::write_frame(send, &FileOp::Done { id }.into_frame(seq)).await?;
    send.finish().map_err(|e| anyhow::anyhow!("finish: {e}"))?;
    Ok(data.len() as u64)
}

/// Receives a file transfer, writing to `out_dir/<name>`; returns the path.
pub async fn recv_file(recv: &mut RecvStream, out_dir: &Path) -> Result<PathBuf> {
    use tokio::io::{AsyncWriteExt, BufWriter};
    let mut name: Option<String> = None;
    let mut expect_size = 0u64;
    let mut written = 0u64;
    let mut file: Option<BufWriter<tokio::fs::File>> = None;
    loop {
        let f = transport::read_frame(recv).await.context("file frame")?;
        if f.ftype != FrameType::File {
            bail!("non-file frame in file stream");
        }
        match FileOp::from_payload(&f.payload)? {
            FileOp::Meta { name: n, size, .. } => {
                let safe = Path::new(&n)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .with_context(|| format!("bad file name {n:?}"))?;
                expect_size = size;
                name = Some(safe.clone());
                std::fs::create_dir_all(out_dir).ok();
                file = Some(BufWriter::new(
                    tokio::fs::File::create(out_dir.join(&safe)).await.with_context(|| format!("create {safe}"))?,
                ));
            }
            FileOp::Chunk { data, offset, .. } => {
                let w = file.as_mut().context("chunk before meta")?;
                ensure!(offset == written, "chunk offset {offset} != written {written}");
                w.write_all(&data).await.context("write chunk")?;
                written += data.len() as u64;
            }
            FileOp::Done { .. } => break,
            FileOp::Req { .. } => bail!("unexpected Req in recv"),
        }
    }
    ensure!(written == expect_size, "size mismatch: got {written}, meta said {expect_size}");
    let mut f = file.context("no meta received")?;
    f.flush().await.context("flush")?;
    let path = out_dir.join(name.context("no meta")?);
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn op_codec_roundtrip() {
        let ops = vec![
            FileOp::Meta { id: 7, name: "鎶ュ憡.txt".into(), size: 12345 },
            FileOp::Chunk { id: 7, offset: 262144, data: vec![9, 8, 7, 6] },
            FileOp::Done { id: 7 },
            FileOp::Req { id: 9, path: "C:\\tmp\\a.bin".into() },
        ];
        for o in ops {
            let back = FileOp::from_payload(&o.to_payload()).unwrap();
            assert_eq!(o, back);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn file_transfer_over_quic() {
        // 1.2MB pseudo-random file -> stream -> receive -> byte-compare
        let mut content: Vec<u8> = Vec::with_capacity(1_250_000);
        let mut x = 0x1234_5678u32;
        while content.len() < 1_250_000 {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            content.push((x >> 16) as u8);
        }
        let src = std::env::temp_dir().join("lee-test-src.bin");
        let out_dir = std::env::temp_dir().join("lee-test-out");
        std::fs::write(&src, &content).unwrap();

        let (sc, ca) = transport::gen_server_config().unwrap();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let mut server = transport::bind_server(addr, sc).unwrap();
        let bound = server.local_addr().unwrap();

        let src_sender = src.clone();
        let mut sender = tokio::spawn(async move {
            let conn = server.accept().await.expect("server accept conn").await.expect("server hs");
            eprintln!("[server] conn accepted");
            let (mut send, mut r) = conn.accept_bi().await.expect("server accept_bi");
            eprintln!("[server] bidi stream accepted");
            // server waits for the client's request, then pushes the file
            let req = transport::read_frame(&mut r).await.expect("server read Req");
            eprintln!("[server] got Req frame type={:?}", req.ftype);
            let FileOp::Req { path, .. } = FileOp::from_payload(&req.payload).unwrap() else {
                panic!("expected Req");
            };
            assert_eq!(path, src_sender.to_string_lossy());
            let n = send_file(&src_sender, &mut send).await.expect("server send_file");
            eprintln!("[server] send_file done {n} bytes");
            // keep conn+endpoint alive until the client closes (else dropping the
            // server task resets the connection and the client loses buffered data)
            let _ = conn.closed().await;
            n
        });

        let client = transport::client_endpoint(&ca).unwrap();
        let conn = client.connect(bound, transport::SNI).unwrap().await.unwrap();
        // open_bi allocates locally; the first write activates it so the server's
        // accept_bi() fires. Send a Req to request the file (real protocol direction).
        let (mut s, mut recv) = conn.open_bi().await.unwrap();
        let req = FileOp::Req { id: 1, path: src.to_string_lossy().into_owned() };
        transport::write_frame(&mut s, &req.into_frame(0)).await.unwrap();
        let got = tokio::time::timeout(std::time::Duration::from_secs(20), recv_file(&mut recv, &out_dir))
            .await
            .expect("recv_file timeout")
            .unwrap();

        // client done receiving: close so the server's conn.closed() returns
        conn.close(0u32.into(), b"client-done");
        let sent = tokio::time::timeout(std::time::Duration::from_secs(20), &mut sender)
            .await
            .expect("sender timeout")
            .unwrap();
        assert_eq!(sent, content.len() as u64);
        assert_eq!(std::fs::read(&got).unwrap(), content);
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_dir_all(&out_dir);
    }
}

