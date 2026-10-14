//! Pipe tasks use Tokio cancellation and per-write deadlines; owner kills/reaps
//! the child on every exit. Linux also uses PDEATHSIG, Windows a kill-on-close Job.
use super::{protocol::{Frame, Request}, super::config::Agentd};
use anyhow::{Context, Result};
use tokio::{io::{AsyncBufReadExt, AsyncWriteExt, BufReader}, process::{Child, Command}, sync::mpsc};
use std::{process::Stdio, time::Duration, sync::{Arc, atomic::{AtomicUsize, Ordering}}};

pub const WRITER_BACKPRESSURE: &str = "AgentdWriterBackpressure";

pub struct Process {
    child: Child,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    pub writer: mpsc::Sender<Vec<u8>>,
    pub incoming: mpsc::Receiver<Result<Frame>>,
    pub pid: u32,
    write_failure: Arc<parking_lot::Mutex<Option<String>>>,
    outstanding_writes: Arc<AtomicUsize>,
    shutdown_timeout: Duration,
    #[cfg(windows)]
    _job: super::windows_job::Job,
}
impl Process {
    pub fn spawn(config: &Agentd) -> Result<Self> {
        anyhow::ensure!(!config.entry.as_os_str().is_empty(), "AgentdEntryMissing: configure agentd.entry / POLARIS_AGENTD_ENTRY");
        let mut command = Command::new(&config.node);
        command.args(&config.node_args).arg(&config.entry).args(&config.args)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        if let Some(path) = crate::provider::credential_file::provider_store_path() {
            let settings = crate::provider::pi_config::load().map_err(anyhow::Error::msg)?;
            command.env("POLARIS_PROVIDER_STORE", &path)
                .env("POLARIS_CREDENTIAL_LOCK_TIMEOUT_MS", settings.lock_timeout_ms.to_string())
                .env("POLARIS_CREDENTIAL_LOCK_MAX_HOLD_MS", settings.lock_max_hold_ms.to_string())
                .env("POLARIS_CREDENTIAL_LOCK_POLL_MS", settings.lock_poll_ms.to_string());
            if let Some(dir) = path.parent() { command.env("POLARIS_USAGE_DIR", dir.join("pi-usage")); }
        }
        crate::runtime::procs::no_window(command.as_std_mut());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }
        #[cfg(target_os = "linux")]
        {
            let parent = std::process::id();
            unsafe {
                command.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 { return Err(std::io::Error::last_os_error()); }
                    if libc::getppid() != parent as i32 { return Err(std::io::Error::other("parent exited")); }
                    Ok(())
                });
            }
        }
        let mut child = command.spawn().context("AgentdSpawnFailed")?;
        let pid = child.id().context("AgentdPidMissing")?;
        #[cfg(windows)]
        let job = super::windows_job::Job::attach(pid)?;
        let stdout = child.stdout.take().context("AgentdStdoutMissing")?;
        let stderr = child.stderr.take().context("AgentdStderrMissing")?;
        let mut stdin = child.stdin.take().context("AgentdStdinMissing")?;
        let (tx, incoming) = mpsc::channel(config.channel_capacity);
        let limit = config.max_line_bytes;
        let output = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                let frame = read_line(&mut reader, limit).await.and_then(|line| Frame::parse(&line));
                let failed = frame.is_err();
                if tx.send(frame).await.is_err() || failed { break; }
            }
        });
        let logging = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            while let Ok(line) = read_line(&mut reader, limit).await {
                eprintln!("{}", serde_json::json!({"component":"agentd.stderr", "pid":pid,
                    "line":String::from_utf8_lossy(&line)}));
            }
        });
        let (writer, mut rx) = mpsc::channel::<Vec<u8>>(config.channel_capacity);
        let write_failure = Arc::new(parking_lot::Mutex::new(None));
        let failure = write_failure.clone();
        let outstanding_writes = Arc::new(AtomicUsize::new(0));
        let outstanding = outstanding_writes.clone();
        let timeout = Duration::from_millis(config.request_timeout_ms);
        let writing = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                let error = match tokio::time::timeout(timeout, stdin.write_all(&line)).await {
                    Ok(Ok(())) => { outstanding.fetch_sub(1, Ordering::SeqCst); continue; }
                    Ok(Err(error)) => format!("AgentdWriterClosed: {error}"),
                    Err(_) => WRITER_BACKPRESSURE.into(),
                };
                *failure.lock() = Some(error);
                break;
            }
        });
        Ok(Self { child, tasks: vec![output, logging, writing], writer, incoming, pid,
            write_failure, outstanding_writes, shutdown_timeout: Duration::from_millis(config.shutdown_timeout_ms),
            #[cfg(windows)] _job: job })
    }
    pub fn write(&self, request: &Request, max: usize) -> Result<()> {
        if let Some(error) = self.write_error() { anyhow::bail!(error); }
        let mut bytes = serde_json::to_vec(request)?;
        anyhow::ensure!(bytes.len() <= max, "AgentdLineTooLong: outgoing request");
        bytes.push(b'\n');
        self.outstanding_writes.fetch_add(1, Ordering::SeqCst);
        self.writer.try_send(bytes).map_err(|error| {
            self.outstanding_writes.fetch_sub(1, Ordering::SeqCst);
            anyhow::anyhow!(match error {
                mpsc::error::TrySendError::Full(_) => WRITER_BACKPRESSURE,
                mpsc::error::TrySendError::Closed(_) => "AgentdWriterClosed",
            })
        })
    }
    pub fn write_error(&self) -> Option<String> { self.write_failure.lock().clone() }
    pub fn writes_pending(&self) -> bool { self.outstanding_writes.load(Ordering::SeqCst) > 0 }
    pub fn exited(&mut self) -> Result<bool> { Ok(self.child.try_wait()?.is_some()) }
    pub async fn stop(&mut self) {
        for task in &self.tasks { task.abort(); }
        #[cfg(unix)]
        unsafe { libc::killpg(self.pid as i32, libc::SIGKILL); }
        let _ = self.child.start_kill();
        let _ = tokio::time::timeout(self.shutdown_timeout, self.child.wait()).await;
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        for task in &self.tasks { task.abort(); }
        #[cfg(unix)]
        unsafe { libc::killpg(self.pid as i32, libc::SIGKILL); }
        let _ = self.child.start_kill();
    }
}
async fn read_line<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R, max: usize) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    loop {
        let bytes = reader.fill_buf().await?;
        anyhow::ensure!(!bytes.is_empty(), "AgentdEof");
        let end = bytes.iter().position(|b| *b == b'\n');
        let count = end.map(|n| n + 1).unwrap_or(bytes.len());
        anyhow::ensure!(line.len() + count <= max + 1, "AgentdLineTooLong");
        line.extend_from_slice(&bytes[..count]);
        reader.consume(count);
        if end.is_some() { line.pop(); return Ok(line); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::protocol::{Command, Empty, Id};
    #[test]
    fn full_and_closed_writer_have_distinct_errors() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let mut config = Agentd::default();
            config.entry = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-agentd.mjs");
            config.channel_capacity = 1;
            let mut process = Process::spawn(&config).unwrap();
            let request = Request { id: Id::Text("test".into()), command: Command::Ping(Empty {}) };
            // No await: the writer cannot drain the first queued request yet.
            process.write(&request, config.max_line_bytes).unwrap();
            assert_eq!(process.write(&request, config.max_line_bytes).unwrap_err().to_string(), WRITER_BACKPRESSURE);
            process.stop().await;
            tokio::task::yield_now().await;
            assert_eq!(process.write(&request, config.max_line_bytes).unwrap_err().to_string(), "AgentdWriterClosed");
        });
    }
}
