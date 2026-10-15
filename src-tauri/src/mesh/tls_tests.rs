//! Both mesh constructors must consume the authority CA, including long polling.
use super::*;
use std::{fs, io::{Read, Write}, net::TcpListener, process::Command, sync::Arc};
use ureq::rustls::{self, pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject}};

type TestResult = Result<(), Box<dyn std::error::Error>>;
#[test]
fn ordinary_and_watch_clients_share_private_ca_policy() -> TestResult {
    let dir = std::env::temp_dir().join(format!("p7-mesh-tls-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let config_path = dir.join("interconnect.json");
    let previous = std::env::var_os("POLARIS_INTERCONNECT_CONFIG");
    struct Restore(Option<std::ffi::OsString>, std::path::PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            match &self.0 {
                Some(value) => std::env::set_var("POLARIS_INTERCONNECT_CONFIG", value),
                None => std::env::remove_var("POLARIS_INTERCONNECT_CONFIG"),
            }
            let _ = fs::remove_dir_all(&self.1);
        }
    }
    let _restore = Restore(previous, dir.clone());
    std::env::set_var("POLARIS_INTERCONNECT_CONFIG", &config_path);
    fs::write(&config_path, json!({"authority":{"extraCaPem":"/missing/p7-mesh-ca.pem"}}).to_string())?;
    assert!(transport::agent().is_err());
    assert!(security::watch_agent(1).is_err());
    let openssl = |args: &[&str]| -> TestResult {
        let output = Command::new("openssl").args(args).current_dir(&dir).output()?;
        if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).into_owned().into()); }
        Ok(())
    };
    openssl(&["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=Mesh Test CA",
        "-addext", "basicConstraints=critical,CA:TRUE", "-keyout", "ca.key", "-out", "ca.pem"])?;
    openssl(&["req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=localhost",
        "-keyout", "server.key", "-out", "server.csr"])?;
    fs::write(dir.join("extensions"), "subjectAltName=DNS:localhost\nextendedKeyUsage=serverAuth\nbasicConstraints=critical,CA:FALSE\n")?;
    openssl(&["x509", "-req", "-days", "1", "-in", "server.csr", "-CA", "ca.pem", "-CAkey", "ca.key",
        "-CAcreateserial", "-extfile", "extensions", "-out", "server.pem"])?;
    let certificates = CertificateDer::pem_slice_iter(&fs::read(dir.join("server.pem"))?).collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_slice(&fs::read(dir.join("server.key"))?)?;
    let tls = Arc::new(rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()?.with_no_client_auth().with_single_cert(certificates, key)?);
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let server = std::thread::spawn(move || -> Result<(), String> {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut attempts = 0;
        while attempts < 6 && std::time::Instant::now() < deadline {
            let (socket, _) = match listener.accept() {
                Ok(socket) => socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5)); continue;
                }
                Err(error) => return Err(error.to_string()),
            };
            attempts += 1;
            socket.set_read_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
            socket.set_write_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
            let connection = rustls::ServerConnection::new(tls.clone()).map_err(|e| e.to_string())?;
            let mut stream = rustls::StreamOwned::new(connection, socket);
            if stream.read(&mut [0; 4096]).is_ok() {
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            }
        }
        if attempts != 6 { return Err("missing TLS attempts".into()); }
        Ok(())
    });
    for trusted in [false, true] {
        let ca = trusted.then(|| dir.join("ca.pem"));
        fs::write(&config_path, json!({"authority":{"extraCaPem":ca}}).to_string())?;
        for client in [transport::agent()?, security::watch_agent(1)?] {
            assert_eq!(client.get(&format!("https://localhost:{port}")).call().is_ok(), trusted);
            if trusted {
                let error = client.get(&format!("https://127.0.0.1:{port}")).call().err().ok_or("hostname mismatch accepted")?;
                assert!(cloud_err(error).contains("InterconnectTlsCertificateError"));
            }
        }
    }
    server.join().map_err(|_| "TLS server panicked")??;
    Ok(())
}
