//! Loopback mTLS helpers for the outbox receiver rehearsal.

use crate::error::{EngineError, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub fn server_config(cert: &Path, key: &Path, client_ca: &Path) -> Result<ServerConfig> {
    install_crypto_provider();
    let certs = load_certs(cert)?;
    let key = load_key(key)?;
    let mut roots = RootCertStore::empty();
    for ca in load_certs(client_ca)? {
        roots
            .add(ca)
            .map_err(|error| EngineError::Message(format!("invalid TLS client CA: {error}")))?;
    }
    if roots.is_empty() {
        return Err(EngineError::Message(
            "TLS client CA file does not contain a certificate".into(),
        ));
    }
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .map_err(|error| EngineError::Message(format!("TLS client verifier: {error}")))?;
    let mut config = ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .map_err(|error| EngineError::Message(format!("TLS server certificate: {error}")))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

pub struct TlsIncoming {
    listener: TcpListener,
    acceptor: TlsAcceptor,
}

impl TlsIncoming {
    pub fn new(listener: TcpListener, config: ServerConfig) -> Self {
        Self {
            listener,
            acceptor: TlsAcceptor::from(Arc::new(config)),
        }
    }
}

impl axum::serve::Listener for TlsIncoming {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, addr) = match self.listener.accept().await {
                Ok(accepted) => accepted,
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            };
            match self.acceptor.accept(stream).await {
                Ok(tls) => return (tls, addr),
                Err(_) => continue,
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let file = File::open(path).map_err(|error| {
        EngineError::Message(format!(
            "unable to read TLS certificate {}: {error}",
            path.display()
        ))
    })?;
    let mut reader = BufReader::new(file);
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<_, _>>()
        .map_err(|error| {
            EngineError::Message(format!(
                "invalid TLS certificate {}: {error}",
                path.display()
            ))
        })?;
    if certs.is_empty() {
        return Err(EngineError::Message(format!(
            "TLS certificate {} does not contain a PEM certificate",
            path.display()
        )));
    }
    Ok(certs)
}

fn load_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    let file = File::open(path).map_err(|error| {
        EngineError::Message(format!(
            "unable to read TLS key {}: {error}",
            path.display()
        ))
    })?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader)
        .map_err(|error| {
            EngineError::Message(format!("invalid TLS key {}: {error}", path.display()))
        })?
        .ok_or_else(|| {
            EngineError::Message(format!(
                "TLS key {} does not contain a PEM private key",
                path.display()
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_pem_files_fail_closed() {
        assert!(server_config(
            Path::new("missing-cert.pem"),
            Path::new("missing-key.pem"),
            Path::new("missing-ca.pem"),
        )
        .is_err());
    }
}
