//! Pipe tasks use Tokio cancellation and per-write deadlines; owner kills/reaps
//! the child on every exit. Linux also uses PDEATHSIG, Windows a kill-on-close Job.
use super::{protocol::{Frame, Request}, super::config::Agentd};
use anyhow::{Context, Result};
use tokio::{io::{AsyncBufReadExt, AsyncWriteExt, BufReader}, process::{Child, Command}, sync::mpsc};
use std::{process::Stdio, time::Duration};

pub struct Process {
    child: Child,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    pub writer: mpsc::Sender<Vec<u8>>,
    pub incoming: mpsc::Receiver<Result<Frame>>,
    pub pid: u32,
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
        let timeout = Duration::from_millis(config.request_timeout_ms);
        let writing = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if !matches!(tokio::time::timeout(timeout, stdin.write_all(&line)).await, Ok(Ok(()))) { break; }
            }
        });
        Ok(Self { child, tasks: vec![output, logging, writing], writer, incoming, pid, shutdown_timeout: Duration::from_millis(config.shutdown_timeout_ms),
            #[cfg(windows)] _job: job })
    }
    pub fn write(&self, request: &Request, max: usize) -> Result<()> {
        let mut bytes = serde_json::to_vec(request)?;
        anyhow::ensure!(bytes.len() <= max, "AgentdLineTooLong: outgoing request");
        bytes.push(b'\n');
        self.writer.try_send(bytes).map_err(|_| anyhow::anyhow!("AgentdWriterBackpressure"))
    }
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
