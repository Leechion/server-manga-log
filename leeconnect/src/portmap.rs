//! Port mapping (UU feature: port_mapping) — TCP-over-QUIC tunnel.
//! Controller listens on a local TCP port; each accepted connection opens a QUIC
//! bidi stream, which the host relays to its target TCP endpoint. Bytes pump both ways.

use anyhow::{Context, Result};
use quinn::Connection;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const BUF: usize = 64 * 1024;

/// Pump TCP -> QUIC until TCP EOF, then finish the stream.
async fn pump_tcp_to_quic(mut tr: tokio::net::tcp::OwnedReadHalf, mut qs: quinn::SendStream) {
    let mut buf = vec![0u8; BUF];
    loop {
        match tr.read(&mut buf).await {
            Ok(0) | Err(_) => { let _ = qs.finish(); break; }
            Ok(n) => { if qs.write_all(&buf[..n]).await.is_err() { break; } }
        }
    }
}

/// Pump QUIC -> TCP until the stream closes.
async fn pump_quic_to_tcp(mut qr: quinn::RecvStream, mut tw: tokio::net::tcp::OwnedWriteHalf) {
    let mut buf = vec![0u8; BUF];
    loop {
        match qr.read(&mut buf).await {
            Ok(None) | Ok(Some(0)) | Err(_) => break,
            Ok(Some(k)) => { if tw.write_all(&buf[..k]).await.is_err() { break; } }
        }
    }
}

/// Controller side: accept TCP on `local`, tunnel each connection over `conn`.
pub async fn controller_listen(local: SocketAddr, conn: Connection) -> Result<()> {
    let listener = TcpListener::bind(local).await.with_context(|| format!("bind {local}"))?;
    loop {
        let (tcp, _peer) = listener.accept().await.context("tcp accept")?;
        let c = conn.clone();
        tokio::spawn(async move {
            if let Ok((qs, qr)) = c.open_bi().await {
                let (tr, tw) = tcp.into_split();
                let _ = tokio::join!(pump_tcp_to_quic(tr, qs), pump_quic_to_tcp(qr, tw));
            }
        });
    }
}

/// Host side: for each incoming stream, connect to `target` and relay.
pub async fn host_serve(conn: Connection, target: SocketAddr) -> Result<()> {
    loop {
        let (qs, qr) = conn.accept_bi().await.context("accept_bi")?;
        tokio::spawn(async move {
            if let Ok(tcp) = TcpStream::connect(target).await {
                let (tr, tw) = tcp.into_split();
                let _ = tokio::join!(pump_tcp_to_quic(tr, qs), pump_quic_to_tcp(qr, tw));
            }
        });
    }
}

/// Controller side bound to a specific port, returning once listening.
pub async fn controller_listen_ready(local: SocketAddr, conn: Connection) -> Result<()> {
    controller_listen(local, conn).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport;
    use std::net::{IpAddr, Ipv4Addr};

    /// Echo TCP server: replies "echo:<line>\n".
    async fn echo_server(listener: TcpListener) {
        loop {
            let (mut s, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = [0u8; 256];
                if let Ok(n) = s.read(&mut buf).await {
                    let line = String::from_utf8_lossy(&buf[..n]).trim_end().to_string();
                    let resp = format!("echo:{line}\n");
                    let _ = s.write_all(resp.as_bytes()).await;
                }
            });
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn portmap_tcp_over_quic() {
        // echo server (real TCP target)
        let echo_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo_listener.local_addr().unwrap();
        tokio::spawn(echo_server(echo_listener));

        // QUIC pair
        let (sc, ca) = transport::gen_server_config().unwrap();
        let mut server =
            transport::bind_server(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0), sc).unwrap();
        let qbound = server.local_addr().unwrap();

        // host relays QUIC streams -> echo_addr
        let host = tokio::spawn(async move {
            let conn = server.accept().await.unwrap().await.unwrap();
            let _ = host_serve(conn, echo_addr).await;
        });

        let client = transport::client_endpoint(&ca).unwrap();
        let conn = client.connect(qbound, transport::SNI).unwrap().await.unwrap();

        // controller: reserve a port, then tunnel there
        let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local = probe.local_addr().unwrap();
        drop(probe);
        let ctrl_conn = conn.clone();
        let ctrl = tokio::spawn(async move {
            for _ in 0..20 {
                if controller_listen(local, ctrl_conn.clone()).await.is_ok() {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        // client -> controller TCP -> QUIC -> host -> echo -> back
        let mut c = TcpStream::connect(local).await.unwrap();
        c.write_all(b"ping\r\n").await.unwrap();
        let mut resp = [0u8; 32];
        let n = c.read(&mut resp).await.unwrap();
        assert_eq!(&resp[..n], b"echo:ping\n");

        ctrl.abort();
        host.abort();
    }
}
