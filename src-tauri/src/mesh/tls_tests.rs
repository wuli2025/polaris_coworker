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
    let version = Command::new("openssl").arg("version").output()?;
    let openssl_version = String::from_utf8_lossy(&version.stdout).trim().to_owned();
    assert!(version.status.success(), "openssl version: {version:?}");
    fs::write(dir.join("fixture.cnf"), include_str!("tls_fixture.cnf"))?;
    let openssl = |args: &[&str]| -> TestResult {
        let output = Command::new("openssl").args(args).current_dir(&dir)
            .env("OPENSSL_CONF", dir.join("fixture.cnf")).output()?;
        if !output.status.success() {
            return Err(format!("openssl={openssl_version}; args={args:?}; output={output:?}").into());
        }
        Ok(())
    };
    openssl(&["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256", "-days", "2",
        "-config", "fixture.cnf", "-extensions", "ca", "-subj", "/CN=Mesh Test CA",
        "-keyout", "ca.key", "-out", "ca.pem"])?;
    openssl(&["req", "-newkey", "rsa:2048", "-nodes", "-sha256", "-config", "fixture.cnf",
        "-subj", "/CN=Mesh Test Server", "-keyout", "server.key", "-out", "server.csr"])?;
    for (section, output) in [("server", "server.pem"), ("mismatch", "mismatch.pem")] {
        openssl(&["x509", "-req", "-sha256", "-days", "2", "-in", "server.csr",
            "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial",
            "-extfile", "fixture.cnf", "-extensions", section, "-out", output])?;
    }
    // Both endpoints use IPv4 literals: DNS order cannot mask TLS verification.
    let (port, server) = serve(&dir, "server.pem", 4)?;
    let (mismatch_port, mismatch_server) = serve(&dir, "mismatch.pem", 2)?;
    for trusted in [false, true] {
        let ca = trusted.then(|| dir.join("ca.pem"));
        fs::write(&config_path, json!({"authority":{"extraCaPem":ca}}).to_string())?;
        for (name, client) in [("ordinary", transport::agent()?), ("watch", security::watch_agent(1)?)] {
            let probe = client.get(&format!("https://127.0.0.1:{port}")).call();
            let details = probe.as_ref().err().map(error_chain).unwrap_or_default();
            assert_eq!(probe.is_ok(), trusted,
                "client={name}; trusted={trusted}; probe={probe:?}; chain={details}; openssl={openssl_version}");
            if let Err(error) = probe {
                assert!(cloud_err(error).contains("InterconnectTlsCertificateError"),
                    "client={name}; trusted={trusted}; chain={details}; openssl={openssl_version}");
            }
            if trusted {
                let probe = client.get(&format!("https://127.0.0.1:{mismatch_port}")).call();
                let details = probe.as_ref().err().map(error_chain).unwrap_or_default();
                assert!(probe.is_err(),
                    "hostname mismatch accepted: client={name}; trusted={trusted}; probe={probe:?}; openssl={openssl_version}");
                let error = probe.unwrap_err();
                assert!(cloud_err(error).contains("InterconnectTlsCertificateError"),
                    "client={name}; trusted={trusted}; chain={details}; openssl={openssl_version}");
            }
        }
    }
    server.join().map_err(|_| "TLS server panicked")??;
    mismatch_server.join().map_err(|_| "mismatch TLS server panicked")??;
    Ok(())
}

fn error_chain(error: &ureq::Error) -> String {
    let mut chain = Vec::new();
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(error) = cause {
        chain.push(format!("{error} ({error:?})"));
        cause = error.source();
    }
    chain.join(" -> ")
}

type Server = (u16, std::thread::JoinHandle<Result<(), String>>);
fn serve(dir: &std::path::Path, certificate: &str, expected: usize) -> Result<Server, Box<dyn std::error::Error>> {
    let certificates = CertificateDer::pem_slice_iter(&fs::read(dir.join(certificate))?).collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_slice(&fs::read(dir.join("server.key"))?)?;
    let tls = Arc::new(rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()?.with_no_client_auth().with_single_cert(certificates, key)?);
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("TLS fixture {certificate}: bind 127.0.0.1:0: {error:?}"))?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let server = std::thread::spawn(move || -> Result<(), String> {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut attempts = 0;
        while attempts < expected && std::time::Instant::now() < deadline {
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
        if attempts != expected {
            return Err(format!("missing TLS attempts: received {attempts}, expected {expected}"));
        }
        Ok(())
    });
    Ok((port, server))
}
