//! QUIC transport (quinn 0.11) carrying `Frame`s over streams.
//! One bidirectional stream per logical channel (video/input/file/...).
//! Self-signed CA + leaf generated locally (rcgen 0.13); client pins the CA,
//! verifies via SNI "localhost".

use anyhow::{Context, Result};
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, ServerConfig, TransportConfig};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use crate::frame::{Frame, HEADER_LEN};

pub const SNI: &str = "localhost";

/// Generate a throwaway CA + server leaf. Returns (server_config, ca_cert_der).
pub fn gen_server_config() -> Result<(ServerConfig, Vec<u8>)> {
    gen_server_config_extra(&[])
}

/// Like `gen_server_config`, but the leaf also carries extra SANs (rcgen treats
/// IP-shaped strings as IpAddress SANs), so a controller can connect to the
/// host's LAN address with full chain + name verification.
pub fn gen_server_config_extra(extra_sans: &[String]) -> Result<(ServerConfig, Vec<u8>)> {
    use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose};

    // CA
    let mut ca_params = CertificateParams::default();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "LeeConnect Test CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_kp = KeyPair::generate().context("ca keypair")?;
    let ca_cert = ca_params.self_signed(&ca_kp).context("ca self-sign")?;

    // leaf signed by CA, DNS SAN localhost (+ extra LAN IPs for cross-machine)
    let mut sans = vec![SNI.to_string()];
    sans.extend(extra_sans.iter().cloned());
    let leaf_params = CertificateParams::new(sans).context("leaf params")?;
    let leaf_kp = KeyPair::generate().context("leaf keypair")?;
    let leaf_cert = leaf_params
        .signed_by(&leaf_kp, &ca_cert, &ca_kp)
        .context("leaf sign")?;

    let certs = vec![rustls::pki_types::CertificateDer::from(leaf_cert.der().to_vec())];
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(leaf_kp.serialize_der().into());

    // quinn's convenience builder wires ring + TLS1.3 + random token key for us
    let mut sc = ServerConfig::with_single_cert(certs, key).context("server config")?;

    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(5)));
    transport.max_concurrent_bidi_streams(quinn::VarInt::from_u32(64));
    sc.transport_config(Arc::new(transport));
    Ok((sc, ca_cert.der().to_vec()))
}

/// Path where the host publishes its CA so the controller can pin the same root.
pub fn ca_file() -> std::path::PathBuf {
    std::path::Path::new("target/leeconnect-ca.der").to_path_buf()
}

/// Host: generate config and persist the CA cert for the controller to trust.
pub fn gen_server_config_persisted() -> Result<(ServerConfig, Vec<u8>)> {
    gen_server_config_persisted_extra(&[])
}

/// Same, with extra leaf SANs (LAN IPs) for cross-machine pairing.
pub fn gen_server_config_persisted_extra(extra_sans: &[String]) -> Result<(ServerConfig, Vec<u8>)> {
    let (sc, ca_der) = gen_server_config_extra(extra_sans)?;
    if let Some(dir) = ca_file().parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(ca_file(), &ca_der).context("write ca")?;
    Ok((sc, ca_der))
}

/// Controller: load the host's published CA cert (DER).
pub fn load_ca() -> Result<Vec<u8>> {
    std::fs::read(ca_file()).with_context(|| format!("read {}", ca_file().display()))
}

/// Client config pinning our CA.
pub fn gen_client_config(ca_der: &[u8]) -> Result<ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(ca_der.to_vec()))
        .context("add ca root")?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .context("tls versions")?
        .with_root_certificates(roots)
        .with_no_client_auth();
    // no ALPN (server built via with_single_cert sets none; must match)
    let cc = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).context("quic client tls")?,
    ));
    Ok(cc)
}

pub fn bind_server(addr: SocketAddr, cfg: ServerConfig) -> Result<Endpoint> {
    Endpoint::server(cfg, addr).context("bind server")
}

/// Bind a QUIC server endpoint onto an existing std `UdpSocket` (e.g. after STUN/hole-punching).
pub fn bind_server_with_socket(sock: std::net::UdpSocket, cfg: ServerConfig) -> Result<Endpoint> {
    Endpoint::new(
        quinn::EndpointConfig::default(),
        Some(cfg),
        sock,
        Arc::new(quinn::TokioRuntime),
    )
    .context("bind server with socket")
}

pub fn client_endpoint(ca_der: &[u8]) -> Result<Endpoint> {
    let mut ep =
        Endpoint::client(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)).context("bind client")?;
    ep.set_default_client_config(gen_client_config(ca_der)?);
    Ok(ep)
}

/// Bind a QUIC client endpoint onto an existing std `UdpSocket` (e.g. after STUN/hole-punching).
pub fn client_endpoint_with_socket(sock: std::net::UdpSocket, ca_der: &[u8]) -> Result<Endpoint> {
    let mut ep = Endpoint::new(
        quinn::EndpointConfig::default(),
        None,
        sock,
        Arc::new(quinn::TokioRuntime),
    )
    .context("bind client with socket")?;
    ep.set_default_client_config(gen_client_config(ca_der)?);
    Ok(ep)
}

/// Write one frame to a send stream (header + payload, atomic send_all).
pub async fn write_frame(s: &mut SendStream, f: &Frame) -> Result<()> {
    let b = f.encode();
    s.write_all(&b).await.context("write frame")?;
    Ok(())
}

/// Read one frame from a recv stream.
pub async fn read_frame(r: &mut RecvStream) -> Result<Frame> {
    let mut hdr = [0u8; HEADER_LEN];
    r.read_exact(&mut hdr).await.context("read header")?;
    let len = u32::from_be_bytes([hdr[13], hdr[14], hdr[15], hdr[16]]) as usize;
    let mut payload = vec![0u8; len];
    if len > 0 {
        r.read_exact(&mut payload).await.context("read payload")?;
    }
    let (frame, _) =
        Frame::decode(&[&hdr[..], &payload[..]].concat()).map_err(|e| anyhow::anyhow!("decode: {e}"))?;
    Ok(frame)
}

/// Accept the next bidi stream and return it.
pub async fn accept_bidi(conn: &Connection) -> Result<(SendStream, RecvStream)> {
    Ok(conn.accept_bi().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::FrameType;

    #[tokio::test]
    async fn quic_loopback_frames() -> Result<()> {
        let (sc, ca_der) = gen_server_config()?;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let server = bind_server(addr, sc)?;
        let bound = server.local_addr()?;

        let accept = tokio::spawn(async move {
            let conn = server.accept().await.expect("accept").await.expect("handshake");
            let (mut send, mut recv) = conn.accept_bi().await.expect("accept_bi");
            let mut got = Vec::new();
            for _ in 0..3 {
                got.push(read_frame(&mut recv).await.expect("read_frame"));
            }
            // echo a control ack back, then finish so drop doesn't reset the stream
            write_frame(&mut send, &Frame::new(FrameType::Control, 0, 0, b"ack".to_vec()))
                .await
                .expect("ack");
            send.finish().expect("finish ack stream");
            // keep the connection alive until the client closes it (endpoint drop
            // would otherwise reset the connection before the ack is read)
            let _ = conn.closed().await;
            got
        });

        let client = client_endpoint(&ca_der)?;
        let conn = client
            .connect(bound, SNI)?
            .await
            .context("connect")?;
        let (mut send, mut recv) = conn.open_bi().await?;

        let sent = vec![
            Frame::new(FrameType::Video, 1, 100, vec![0xAB; 4096]),
            Frame::new(FrameType::Input, 2, 101, vec![1, 2, 3]),
            Frame::new(FrameType::File, 3, 102, vec![0u8; 65536]),
        ];
        for f in &sent {
            write_frame(&mut send, f).await?;
        }
        send.finish()?;

        // read the ack BEFORE closing anything
        let ack = read_frame(&mut recv).await?;
        assert_eq!(ack.ftype, FrameType::Control);
        assert_eq!(ack.payload, b"ack");

        // now close so the server task (waiting on conn.closed()) can finish
        conn.close(0u32.into(), b"done");
        let got = accept.await?;
        assert_eq!(got, sent);
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_streams() -> Result<()> {
        let (sc, ca_der) = gen_server_config()?;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let server = bind_server(addr, sc)?;
        let bound = server.local_addr()?;

        let accept = tokio::spawn(async move {
            let conn = server.accept().await.expect("accept").await.expect("hs");
            let mut sum = 0usize;
            for _ in 0..4 {
                let (_, mut recv) = conn.accept_bi().await.expect("accept_bi");
                let f = read_frame(&mut recv).await.expect("frame");
                sum += f.payload.len();
            }
            sum
        });

        let client = client_endpoint(&ca_der)?;
        let conn = client.connect(bound, SNI)?.await?;
        let mut handles = Vec::new();
        for i in 0..4u8 {
            let c = conn.clone();
            handles.push(tokio::spawn(async move {
                let (mut s, _) = c.open_bi().await.unwrap();
                write_frame(&mut s, &Frame::new(FrameType::Video, i as u16, i as u32, vec![i; 1000]))
                    .await
                    .unwrap();
                s.finish().unwrap();
            }));
        }
        for h in handles {
            h.await.ok();
        }
        assert_eq!(accept.await?, 4000);
        Ok(())
    }
}
