//! Test-only PEM mint for loopback mTLS. Included from `#[cfg(test)]` modules.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct MtlsPems {
    pub ca: PathBuf,
    pub server_cert: PathBuf,
    pub server_key: PathBuf,
    pub client_cert: PathBuf,
    pub client_key: PathBuf,
}

fn openssl() -> PathBuf {
    let git = PathBuf::from(r"C:\Program Files\Git\usr\bin\openssl.exe");
    if git.is_file() {
        return git;
    }
    PathBuf::from("openssl")
}

fn run(args: &[&str]) {
    let status = Command::new(openssl())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("openssl must be available for mTLS tests");
    assert!(status.success(), "openssl {args:?} failed");
}

pub fn write_into(dir: &Path) -> MtlsPems {
    let ca_ext = dir.join("ca.ext");
    let server_ext = dir.join("server.ext");
    let client_ext = dir.join("client.ext");
    std::fs::write(
        &ca_ext,
        "basicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n",
    )
    .unwrap();
    std::fs::write(
        &server_ext,
        "subjectAltName=IP:127.0.0.1,DNS:localhost\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n",
    )
    .unwrap();
    std::fs::write(
        &client_ext,
        "keyUsage=critical,digitalSignature\nextendedKeyUsage=clientAuth\n",
    )
    .unwrap();

    let pems = MtlsPems {
        ca: dir.join("ca.pem"),
        server_cert: dir.join("server.pem"),
        server_key: dir.join("server.key"),
        client_cert: dir.join("client.pem"),
        client_key: dir.join("client.key"),
    };
    let ca_key = dir.join("ca.key");
    let server_csr = dir.join("server.csr");
    let client_csr = dir.join("client.csr");

    run(&[
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-days",
        "1",
        "-subj",
        "/CN=tn10-test-ca",
        "-addext",
        "basicConstraints=critical,CA:TRUE",
        "-addext",
        "keyUsage=critical,keyCertSign,cRLSign",
        "-keyout",
        ca_key.to_str().unwrap(),
        "-out",
        pems.ca.to_str().unwrap(),
    ]);
    run(&[
        "req",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-subj",
        "/CN=127.0.0.1",
        "-keyout",
        pems.server_key.to_str().unwrap(),
        "-out",
        server_csr.to_str().unwrap(),
    ]);
    run(&[
        "x509",
        "-req",
        "-in",
        server_csr.to_str().unwrap(),
        "-CA",
        pems.ca.to_str().unwrap(),
        "-CAkey",
        ca_key.to_str().unwrap(),
        "-CAcreateserial",
        "-out",
        pems.server_cert.to_str().unwrap(),
        "-days",
        "1",
        "-extfile",
        server_ext.to_str().unwrap(),
    ]);
    run(&[
        "req",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-subj",
        "/CN=tn10-outbox",
        "-keyout",
        pems.client_key.to_str().unwrap(),
        "-out",
        client_csr.to_str().unwrap(),
    ]);
    run(&[
        "x509",
        "-req",
        "-in",
        client_csr.to_str().unwrap(),
        "-CA",
        pems.ca.to_str().unwrap(),
        "-CAkey",
        ca_key.to_str().unwrap(),
        "-CAcreateserial",
        "-out",
        pems.client_cert.to_str().unwrap(),
        "-days",
        "1",
        "-extfile",
        client_ext.to_str().unwrap(),
    ]);
    pems
}
