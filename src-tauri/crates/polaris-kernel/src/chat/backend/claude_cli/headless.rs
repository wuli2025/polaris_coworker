//! Headless claude 只读管线 + JSON 决策提取 —— 内核横切能力。
//!
//! 原居 kb/compile.rs(Wave B 基础设施), 分仓 Phase 1 下沉内核: fable(回声蒸馏/文件
//! 中心 LLM/本体抽取/检索 AI 扩写)与 wiki(enrich/dedup)都在用, 而引擎间禁止互引 ——
//! 它只依赖 doctor(定位 claude/加固子环境)与 provider(会话隔离), 天然属于 kernel。
//!
//! 模式(借鉴 llm_wiki「让 AI 只出决策数据, 代码执行改动」): 起一个**只读**
//! (allowedTools 仅 Read/Glob/Grep, 物理上无法写文件)的 headless claude, 让它读资料、
//! 输出一段 JSON 决策, 把全部 assistant 文本收集起来返回; 改文件由 Rust 做。

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg_attr(not(windows), allow(unused_variables))]
fn no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: GUI 进程 spawn 控制台子进程时不弹黑窗
        cmd.creation_flags(0x0800_0000);
    }
}

/// Original CLI flags and parser, with the shared registry for backend cancellation.
pub(super) fn run_readonly<F: FnMut(&str, &str)>(
    root: &Path,
    prompt: &str,
    on_event: F,
    timeout: Option<std::time::Duration>,
    req_id: &str,
) -> Result<String, String> {
    let claude_bin: std::ffi::OsString = crate::doctor::resolve_claude_exe()
        .map(|p| p.into_os_string())
        .unwrap_or_else(|| "claude".into());
    run_command(root, prompt, on_event, timeout, req_id, Command::new(&claude_bin))
}

pub(super) fn run_command<F: FnMut(&str, &str)>(
    root: &Path, prompt: &str, mut on_event: F,
    timeout: Option<std::time::Duration>, req_id: &str, mut cmd: Command,
) -> Result<String, String> {
    use crate::runtime::procs::CHILDREN;
    let deadline = timeout.map(|duration| std::time::Instant::now() + duration);
    let polling = std::time::Duration::from_millis(
        super::super::config::Config::load().map_err(|e| e.to_string())?.agentd.control_poll_ms);
    cmd.args([
        "--print",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-mode=bypassPermissions",
        "--allowedTools",
        "Read,Glob,Grep", // 只读: 物理上不给 Write/Edit, 决策数据落地由 Rust 执行
    ])
    .current_dir(root)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    crate::doctor::harden_child_env(&mut cmd); // loopback NO_PROXY + 清干扰变量
    crate::provider::scope_child_claude(&mut cmd); // 隔离模式第三方 → 私有会话账本
    no_window(&mut cmd);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| format!("调起 claude 失败: {e}"))?;
    let stdin = child.stdin.take();
    let stderr_buf = std::sync::Arc::new(parking_lot::Mutex::new(String::new()));
    if let Some(se) = child.stderr.take() {
        let buf = stderr_buf.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(se)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if !line.trim().is_empty() {
                    buf.lock().push_str(&line);
                    buf.lock().push('\n');
                }
            }
        });
    }

    let stdout = child.stdout.take();

    // 墙钟看门狗:设了 timeout 时,到点 kill 子进程 —— stdout 随之关闭,下面的读循环自然结束。
    // 命令正常读完会 drop done_tx 让看门狗提前醒来(不空等满 timeout);None=不设超时(旧行为)。
    CHILDREN.insert(req_id, child);
    if CHILDREN.take_cancel(req_id) { CHILDREN.kill(req_id); }
    if let Some(mut stdin) = stdin {
        let prompt = prompt.to_owned();
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(prompt.as_bytes());
        });
    }
    let timed_out = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let watchdog = timeout.map(|dur| {
        let request = req_id.to_owned();
        let flag = std::sync::Arc::clone(&timed_out);
        std::thread::spawn(move || {
            if matches!(
                done_rx.recv_timeout(dur),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ) {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                CHILDREN.kill(&request);
            }
        })
    });

    let mut collected = String::new();
    let mut result_err: Option<String> = None;
    if let Some(so) = stdout {
        for line in BufReader::new(so)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
            if ty == "result" {
                if let Some(st) = v.get("subtype").and_then(|x| x.as_str()) {
                    if st.starts_with("error") {
                        result_err = Some(format!("claude 返回错误: {st}"));
                        break;
                    }
                }
                continue;
            }
            if ty != "assistant" {
                continue;
            }
            let Some(content) = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            else {
                continue;
            };
            for block in content {
                match block.get("type").and_then(|x| x.as_str()) {
                    Some("tool_use") => {
                        let name = block.get("name").and_then(|x| x.as_str()).unwrap_or("");
                        on_event("tool", name);
                    }
                    Some("text") => {
                        if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                            collected.push_str(t);
                            on_event("delta", t);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // 读到 EOF(或 error 提前 break):通知看门狗退出,再回收子进程(避免僵尸 + 线程泄漏)。
    drop(done_tx);
    if let Some(h) = watchdog {
        let _ = h.join();
    }
    if result_err.is_some() { CHILDREN.kill(req_id); }
    let status = if let Some(mut child) = CHILDREN.remove(req_id) {
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(Some(status)),
                Err(error) => break Err(error),
                Ok(None) => {
                    let expired = deadline.is_some_and(|end| std::time::Instant::now() >= end);
                    if expired || CHILDREN.take_cancel(req_id) {
                        if expired { timed_out.store(true, std::sync::atomic::Ordering::SeqCst); }
                        crate::runtime::procs::kill_tree(child.id());
                        let _ = child.kill();
                    }
                    std::thread::sleep(polling);
                }
            }
        }
    } else { Ok(None) };
    CHILDREN.take_cancel(req_id);

    if timed_out.load(std::sync::atomic::Ordering::SeqCst) {
        let secs = timeout.map(|d| d.as_secs()).unwrap_or_default();
        return Err(format!("claude 超时({secs}s)已终止"));
    }
    if let Some(e) = result_err {
        return Err(e);
    }
    let success = match &status { Ok(Some(status)) => status.success(), Ok(None) => true, Err(_) => false };
    if !success {
        let se = stderr_buf.lock().clone();
        return Err(format!(
            "claude 异常退出{}",
            if se.is_empty() {
                String::new()
            } else {
                format!(": {se}")
            }
        ));
    }
    Ok(collected)
}

