// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::net::{IpAddr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use rcgen::generate_simple_self_signed;
use remoc::rch;
use rustls::{
    ClientConfig, DigitallySignedStruct, Error, ServerConfig, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
};
use tokio::net::{TcpListener, TcpSocket, TcpStream, UnixListener, UnixSocket};
use tokio_rustls::{TlsAcceptor, TlsConnector};

pub const UI_RCH_PORT: u16 = 7101;
pub const RR_RCH_PORT: u16 = 7102;
pub const VTEP_RCH_PATH: &str = "/run/gvls-vtep.sock";

pub const HELLO_INTERVAL: u64 = 15;
pub const HELLO_TIMEOUT: u64 = 60;

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

pub struct TlsRchListener {
    tcp_listener: TcpListener,
    tls_acceptor: TlsAcceptor,
}

impl TlsRchListener {
    pub async fn new(addr: IpAddr, port: u16) -> Result<Self, String> {
        let cert = match generate_simple_self_signed(vec!["localhost".to_string()]) {
            Ok(cert) => cert,
            Err(e) => return Err(format!("Generate simple self signed error: {e}")),
        };
        let cert_der: CertificateDer<'static> = cert.cert.der().clone();
        let key_der = match PrivateKeyDer::try_from(cert.signing_key.serialize_der()) {
            Ok(key_der) => key_der,
            Err(e) => return Err(format!("Get private key der error: {e}")),
        };
        let server_cfg = match ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], key_der)
        {
            Ok(server_cfg) => server_cfg,
            Err(e) => return Err(format!("Build server config error: {e}")),
        };
        let tcp_listener = match TcpListener::bind((addr, port)).await {
            Ok(tcp_listener) => tcp_listener,
            Err(e) => return Err(format!("TCP listener bind error: {e}")),
        };
        let tls_acceptor = TlsAcceptor::from(Arc::new(server_cfg));
        Ok(Self {
            tcp_listener,
            tls_acceptor,
        })
    }
    pub async fn rch_accept<
        S: std::marker::Send + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static,
        R: std::marker::Send + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static,
    >(
        &mut self,
    ) -> Result<(rch::base::Sender<S>, rch::base::Receiver<R>, IpAddr), String> {
        let (tcp, addr) = match self.tcp_listener.accept().await {
            Ok((tcp, addr)) => (tcp, addr),
            Err(e) => return Err(format!("Listener accept error: {e}")),
        };
        let tls = match self.tls_acceptor.accept(tcp).await {
            Ok(tls) => tls,
            Err(e) => return Err(format!("Acceptor accept error: {e}")),
        };
        let (tls_rx, tls_tx) = tokio::io::split(tls);
        let (conn, tx, rx) = match remoc::Connect::io(remoc::Cfg::default(), tls_rx, tls_tx).await {
            Ok((conn, tx, rx)) => (conn, tx, rx),
            Err(e) => return Err(format!("remoc connect io failed: {e}")),
        };
        tokio::spawn(conn);
        Ok((tx, rx, addr.ip()))
    }
}

pub struct UdsRchListener {
    unix_listener: UnixListener,
}

impl UdsRchListener {
    pub async fn new(path: String) -> Result<Self, String> {
        let sk = match UnixSocket::new_stream() {
            Ok(sk) => sk,
            Err(e) => return Err(format!("New stream error: {e}")),
        };
        if let Ok(res) = std::fs::exists(&path)
            && res == true
        {
            // XXX: 乱暴。
            let _ = std::fs::remove_file(&path);
        }
        if let Err(e) = sk.bind(path) {
            return Err(format!("Bind error: {e}"));
        }
        let unix_listener = match sk.listen(2) {
            Ok(unix_listener) => unix_listener,
            Err(e) => return Err(format!("Listen error: {e}")),
        };
        Ok(Self { unix_listener })
    }
    pub async fn rch_accept<
        S: std::marker::Send + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static,
        R: std::marker::Send + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static,
    >(
        &mut self,
    ) -> Result<(rch::base::Sender<S>, rch::base::Receiver<R>), String> {
        let (uds, _) = match self.unix_listener.accept().await {
            Ok(uds) => uds,
            Err(e) => return Err(format!("Listener accept error: {e}")),
        };
        let (uds_rx, uds_tx) = tokio::io::split(uds);
        let (conn, tx, rx) = match remoc::Connect::io(remoc::Cfg::default(), uds_rx, uds_tx).await {
            Ok((conn, tx, rx)) => (conn, tx, rx),
            Err(e) => return Err(format!("remoc connect io failed: {e}")),
        };
        tokio::spawn(conn);
        Ok((tx, rx))
    }
}

pub async fn rch_connect_addr<
    S: std::marker::Send + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static,
    R: std::marker::Send + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static,
>(
    addr: IpAddr,
    port: u16,
) -> Result<(rch::base::Sender<S>, rch::base::Receiver<R>), String> {
    let client_cfg = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
        .with_no_client_auth();
    let stream = match TcpStream::connect((addr, port)).await {
        Ok(stream) => stream,
        Err(e) => return Err(format!("TCP connect error: {e}")),
    };
    let connector = TlsConnector::from(Arc::new(client_cfg));
    let server_name = match ServerName::try_from(addr) {
        Ok(server_name) => server_name,
        Err(e) => return Err(format!("Get server name error: {e}")),
    };
    let tls = match connector.connect(server_name, stream).await {
        Ok(tls) => tls,
        Err(e) => return Err(format!("Connector connect error: {e}")),
    };
    let (tls_rx, tls_tx) = tokio::io::split(tls);
    let (conn, tx, rx) = match remoc::Connect::io(remoc::Cfg::default(), tls_rx, tls_tx).await {
        Ok((conn, tx, rx)) => (conn, tx, rx),
        Err(e) => return Err(format!("remoc connect io failed: {e}")),
    };
    tokio::spawn(conn);
    Ok((tx, rx))
}

pub async fn rch_connect_host<
    S: std::marker::Send + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static,
    R: std::marker::Send + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static,
>(
    addr: String,
    port: u16,
    loc_addr: Ipv6Addr,
) -> Result<(rch::base::Sender<S>, rch::base::Receiver<R>, Ipv6Addr), String> {
    let client_cfg = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
        .with_no_client_auth();
    let sock_loc = SocketAddr::new(IpAddr::V6(loc_addr), 0);
    let mut sock_rem: Option<SocketAddr> = None;
    let mut rem_addr: Option<Ipv6Addr> = None;
    if let Ok(sock_addrs) = (addr.as_str(), port).to_socket_addrs() {
        for sock_addr in sock_addrs {
            if let SocketAddr::V6(sa) = sock_addr {
                sock_rem = Some(sock_addr);
                rem_addr = Some(sa.ip().clone());
                break;
            }
        }
    }
    if sock_rem.is_none() || rem_addr.is_none() {
        return Err(format!("Remote host {addr} resolve failed"));
    }
    let sock_rem = sock_rem.unwrap();
    let rem_addr = rem_addr.unwrap();
    let socket = match TcpSocket::new_v6() {
        Ok(socket) => socket,
        Err(e) => return Err(format!("TCP socket open error: {e}")),
    };
    if let Err(e) = socket.bind(sock_loc) {
        return Err(format!("TCP socket bind error: {e}"));
    }
    let stream = match socket.connect(sock_rem).await {
        Ok(stream) => stream,
        Err(e) => return Err(format!("TCP connect error: {e}")),
    };
    let connector = TlsConnector::from(Arc::new(client_cfg));
    let server_name = match ServerName::try_from(addr) {
        Ok(server_name) => server_name,
        Err(e) => return Err(format!("Get server name error: {e}")),
    };
    let tls = match connector.connect(server_name, stream).await {
        Ok(tls) => tls,
        Err(e) => return Err(format!("Connector connect error: {e}")),
    };
    let (tls_rx, tls_tx) = tokio::io::split(tls);
    let (conn, tx, rx) = match remoc::Connect::io(remoc::Cfg::default(), tls_rx, tls_tx).await {
        Ok((conn, tx, rx)) => (conn, tx, rx),
        Err(e) => return Err(format!("remoc connect io failed: {e}")),
    };
    tokio::spawn(conn);
    Ok((tx, rx, rem_addr))
}

pub async fn rch_connect_path<
    S: std::marker::Send + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static,
    R: std::marker::Send + serde::Serialize + for<'de> serde::Deserialize<'de> + 'static,
>(
    path: String,
) -> Result<(rch::base::Sender<S>, rch::base::Receiver<R>), String> {
    let sk = match UnixSocket::new_stream() {
        Ok(sk) => sk,
        Err(e) => return Err(format!("New stream error: {e}")),
    };
    let stream = match sk.connect(path).await {
        Ok(stream) => stream,
        Err(e) => return Err(format!("Connect error: {e}")),
    };
    let (uds_rx, uds_tx) = tokio::io::split(stream);
    let (conn, tx, rx) = match remoc::Connect::io(remoc::Cfg::default(), uds_rx, uds_tx).await {
        Ok((conn, tx, rx)) => (conn, tx, rx),
        Err(e) => return Err(format!("remoc connect io failed: {e}")),
    };
    tokio::spawn(conn);
    Ok((tx, rx))
}
