//! Bounded Docker transport. Reads pipes concurrently so a full pipe cannot stall probing.
use std::{io::Read, process::{Command, Stdio}, time::{Duration, Instant}};
use super::host_tools::SandboxOutput;

pub fn run(command: &mut Command, timeout: Duration, limit: usize) -> anyhow::Result<SandboxOutput> {
    let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .map_err(|e| anyhow::anyhow!("SANDBOX_UNAVAILABLE: {e}"))?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("SandboxMissingStdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| anyhow::anyhow!("SandboxMissingStderr"))?;
    let read = move |pipe: Box<dyn Read + Send>| -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        pipe.take((limit + 1) as u64).read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let err_tx = tx.clone();
    std::thread::spawn(move || { let _ = tx.send((true, read(Box::new(stdout)))); });
    std::thread::spawn(move || { let _ = err_tx.send((false, read(Box::new(stderr)))); });
    let start = Instant::now();
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? { break (status, false); }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            break (child.wait()?, true);
        }
        std::thread::sleep(Duration::from_millis(2).min(timeout.saturating_sub(start.elapsed())));
    };
    // A stuck Docker descendant may hold a pipe after the client dies. Never join it on timeout.
    if timed_out {
        return Ok(SandboxOutput { timed_out: true, exit_code: None, signal: None, output: String::new() });
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    for _ in 0..2 {
        match rx.recv_timeout(timeout.saturating_sub(start.elapsed())) {
            Ok((true, bytes)) => stdout = bytes?,
            Ok((false, bytes)) => stderr = bytes?,
            Err(_) => return Ok(SandboxOutput { timed_out: true, ..SandboxOutput::default() }),
        }
    }
    anyhow::ensure!(stdout.len() <= limit && stderr.len() <= limit, "SANDBOX_OUTPUT_LIMIT");
    #[cfg(unix)]
    let signal = { use std::os::unix::process::ExitStatusExt; status.signal() };
    #[cfg(not(unix))]
    let signal = None;
    Ok(SandboxOutput { output: format!("{}{}", String::from_utf8_lossy(&stdout), String::from_utf8_lossy(&stderr)),
        exit_code: status.code().or_else(|| signal.map(|n| 128 + n)), signal, timed_out: false })
}
