use super::*;
use std::{fs, io::{Read, Write}, net::TcpListener, path::PathBuf, process::Command,
    thread, time::{Duration, Instant}};
use rustls::pki_types::PrivateKeyDer;

type TestResult = Result<(), Box<dyn Error>>;
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}
fn openssl(dir: &Directory, args: &[&str]) -> TestResult {
    let result = Command::new("openssl").args(args).current_dir(&dir.0).output()?;
    if !result.status.success() { return Err(String::from_utf8_lossy(&result.stderr).into_owned().into()); }
    Ok(())
}

fn certificate_fixture(label: &str) -> Result<Directory, Box<dyn Error>> {
    let dir = Directory(std::env::temp_dir().join(format!("p7-ca-{label}-{}", std::process::id())));
    fs::create_dir_all(&dir.0)?;
    openssl(&dir, &["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
        "-subj", "/CN=Polaris Test CA", "-addext", "basicConstraints=critical,CA:TRUE",
        "-keyout", "ca.key", "-out", "ca.pem"])?;
    openssl(&dir, &["req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=localhost",
        "-keyout", "server.key", "-out", "server.csr"])?;
    fs::write(dir.0.join("extensions"), "subjectAltName=DNS:localhost\nextendedKeyUsage=serverAuth\nbasicConstraints=critical,CA:FALSE\n")?;
    openssl(&dir, &["x509", "-req", "-days", "1", "-in", "server.csr", "-CA", "ca.pem",
        "-CAkey", "ca.key", "-CAcreateserial", "-extfile", "extensions", "-out", "server.pem"])?;
    Ok(dir)
}

#[test]
fn private_ca_requires_explicit_trust_and_keeps_hostname_verification() -> TestResult {
    let dir = certificate_fixture("https")?;
    let certs = CertificateDer::pem_slice_iter(&fs::read(dir.0.join("server.pem"))?).collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_slice(&fs::read(dir.0.join("server.key"))?)?;
    let config = Arc::new(rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()?.with_no_client_auth().with_single_cert(certs, key)?);
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let server = thread::spawn(move || -> Result<(), String> {
        let end = Instant::now() + Duration::from_secs(20);
        let mut attempts = 0;
        while attempts < 4 && Instant::now() < end {
            let (socket, _) = match listener.accept() {
                Ok(socket) => socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5)); continue;
                }
                Err(e) => return Err(e.to_string()),
            };
            attempts += 1;
            socket.set_read_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
            socket.set_write_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
            let connection = rustls::ServerConnection::new(config.clone()).map_err(|e| e.to_string())?;
            let mut stream = rustls::StreamOwned::new(connection, socket);
            let mut request = [0u8; 4096];
            // Untrusted CA and hostname mismatch must fail the TLS handshake.
            if stream.read(&mut request).is_ok() {
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                    .map_err(|e| e.to_string())?;
            }
        }
        if attempts != 4 { return Err("HTTPS test did not receive all attempts".into()); }
        Ok(())
    });
    let builder = || ureq::AgentBuilder::new().timeout(Duration::from_secs(3)).try_proxy_from_env(false);
    let url = format!("https://localhost:{port}/");
    let error = with_ca(builder(), None)?.get(&url).call().err().ok_or("untrusted CA accepted")?;
    assert!(diagnostic(&error).contains("InterconnectTlsCertificateError"));
    let pem = fs::read_to_string(dir.0.join("ca.pem"))?;
    assert_eq!(with_ca(builder(), Some(&pem))?.get(&url).call()?.into_string()?, "ok");
    let path = dir.0.join("ca.pem");
    let path = path.to_str().ok_or("CA path encoding")?;
    assert_eq!(with_ca(builder(), Some(path))?.get(&url).call()?.into_string()?, "ok");
    let error = with_ca(builder(), Some(&pem))?.get(&format!("https://127.0.0.1:{port}/"))
        .call().err().ok_or("hostname mismatch accepted")?;
    assert!(diagnostic(&error).contains("InterconnectTlsCertificateError"));
    server.join().map_err(|_| "HTTPS server panicked")??;
    assert!(with_ca(builder(), Some("-----BEGIN CERTIFICATE-----\ninvalid\n-----END CERTIFICATE-----")).is_err());
    Ok(())
}

#[test]
fn optional_ca_config_is_typed_and_defaults_to_none() -> TestResult {
    assert!(crate::collab::interconnect::resolve(None, |_| None)?.authority.extra_ca_pem.is_none());
    for invalid in [r#"{"authority":{"extraCaPem":4}}"#, r#"{"authority":{"extraCaPem":" "}}"#] {
        assert!(crate::collab::interconnect::resolve(Some(invalid), |_| None).is_err());
    }
    Ok(())
}

// Exercises the real rustls handshake even in runners that forbid TCP listeners.
#[test]
fn ca_chain_and_hostname_are_verified_without_sockets() -> TestResult {
    let dir = certificate_fixture("memory")?;
    let pem = fs::read_to_string(dir.0.join("ca.pem"))?;
    let certs = CertificateDer::pem_slice_iter(&fs::read(dir.0.join("server.pem"))?).collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_slice(&fs::read(dir.0.join("server.key"))?)?;
    let server = Arc::new(rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()?.with_no_client_auth().with_single_cert(certs, key)?);
    let handshake = |extra: Option<&str>, name: &'static str| -> TestResult {
        let mut client = rustls::ClientConnection::new(Arc::new(tls_config(extra)?), name.try_into()?)?;
        let mut peer = rustls::ServerConnection::new(server.clone())?;
        for _ in 0..10 {
            let mut bytes = Vec::new();
            client.write_tls(&mut bytes)?;
            peer.read_tls(&mut std::io::Cursor::new(bytes))?;
            peer.process_new_packets()?;
            let mut bytes = Vec::new();
            peer.write_tls(&mut bytes)?;
            client.read_tls(&mut std::io::Cursor::new(bytes))?;
            client.process_new_packets()?;
            if !client.is_handshaking() && !peer.is_handshaking() { return Ok(()); }
        }
        Err("TLS handshake did not complete".into())
    };
    assert!(handshake(None, "localhost").is_err());
    handshake(Some(&pem), "localhost")?;
    handshake(Some(dir.0.join("ca.pem").to_str().ok_or("CA path encoding")?), "localhost")?;
    assert!(handshake(Some(&pem), "wrong.example").is_err());
    assert!(with_ca(ureq::AgentBuilder::new(), Some("/missing/p7-ca.pem")).is_err());
    assert!(with_ca(ureq::AgentBuilder::new(), Some("-----BEGIN CERTIFICATE-----\ninvalid\n-----END CERTIFICATE-----")).is_err());
    Ok(())
}
