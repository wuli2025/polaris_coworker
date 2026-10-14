//! Reverse direction of the unit test: a killed Rust owner is reclaimed by Node.
use polaris_kernel::provider::credential_file::FileLock;
use std::{fs, io::{Read, Write}, path::Path, process::{Command, Stdio}};

#[test]
fn rust_holder_probe() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::var_os("R5_LOCK_CHILD") else { return Ok(()); };
    let _lock = FileLock::acquire(Path::new(&path))?;
    println!("LOCK_READY");
    std::io::stdout().flush()?;
    loop { std::thread::park(); }
}
#[cfg(unix)]
#[test]
fn killed_rust_holder_is_recovered_by_node() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("r5-rust-node-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    let path = dir.join("providers.json");
    let mut child = Command::new(std::env::current_exe()?).args(["--exact", "rust_holder_probe", "--nocapture"])
        .env("R5_LOCK_CHILD", &path).stdout(Stdio::piped()).spawn()?;
    let mut stdout = child.stdout.take().ok_or("MissingStdout")?;
    let mut bytes = Vec::new();
    let ready = (|| -> std::io::Result<()> {
        while !bytes.ends_with(b"LOCK_READY\n") {
            let mut byte = [0];
            stdout.read_exact(&mut byte)?;
            bytes.push(byte[0]);
        }
        Ok(())
    })();
    child.kill()?;
    child.wait()?;
    ready?;
    let output = Command::new("node").args(["--import", "tsx", "--input-type=module", "-e",
        "import { FileCredentials } from './src/models/file-credentials.ts'; import { Redactor } from './src/models/credentials.ts'; await new FileCredentials(process.argv[1],new Redactor(),{timeoutMs:1000,pollMs:1}).modify('p',async()=>({type:'api_key',key:'recovered'}));"])
        .arg(&path).current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../agentd")).output()?;
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let disk: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    assert_eq!(disk["credentials"]["p"]["key"], "recovered");
    fs::remove_dir_all(dir)?;
    Ok(())
}
