//! Explicit claude-cli compatibility path; never entered by Pi.
use super::{emit_compile, KbCompileEvent, AppHandle};
use crate::kb::kb_reindex;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Arc;
use polaris_kernel::chat::backend::AgentBackend;
#[cfg(feature = "desktop")]
use tauri::Emitter;
#[cfg_attr(not(windows), allow(unused_variables))]
pub(crate) fn compile_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: GUI 进程 spawn 控制台子进程时不弹黑窗
        cmd.creation_flags(0x0800_0000);
    }
}


pub(super) fn run(app: AppHandle, run_id_thread: String, root: std::path::PathBuf,
    backend: Arc<dyn AgentBackend>, cap_secs: u64) {
    if backend.status().backend != "claude-cli" {
        emit_compile(&app, &run_id_thread, "error", Some("LegacyBackendGuard".into()));
        return;
    }
    let claude_bin: std::ffi::OsString = crate::doctor::resolve_claude_exe()
        .map(|p| p.into_os_string()).unwrap_or_else(|| "claude".into());
    let prompt = super::prompt::compile_directive(&root.to_string_lossy());
    // prompt 经 stdin 喂给 claude (而非命令行参数): 大 prompt 不会撞 Windows 命令行
    // 长度上限, 也不会因 prompt 以 `-` 开头被当成 flag —— 实测 argv 路径在某些 shell 下
    // 会触发 claude 的「Input must be provided」直接退 1, stdin 管道稳。
    let mut cmd = Command::new(&claude_bin);
    cmd.args([
        "--print",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-mode=bypassPermissions",
        "--allowedTools",
        "Read,Write,Edit,Glob,Grep",
    ])
    .current_dir(&root)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    crate::doctor::harden_child_env(&mut cmd); // loopback NO_PROXY + 清干扰变量
    crate::provider::scope_child_claude(&mut cmd); // 隔离模式第三方 → 私有会话账本
    compile_no_window(&mut cmd);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            emit_compile(
                &app,
                &run_id_thread,
                "error",
                Some(format!("调起 claude 失败: {e}")),
            );
            let _ = app.emit(
                "kb:compile",
                KbCompileEvent {
                    run_id: run_id_thread.clone(),
                    kind: "done".into(),
                    text: Some("编译未启动".into()),
                    doc_count: None,
                },
            );
            return;
        }
    };

    // 把 prompt 写进 stdin 并关闭 (drop 即关), claude 读到 EOF 后开始干活
    if let Some(mut si) = child.stdin.take() {
        use std::io::Write as _;
        let _ = si.write_all(prompt.as_bytes());
        // si 在此作用域结束时 drop → stdin 关闭, 触发 claude 开始处理
    }

    // stderr: 累积, 退出非零时给原因 (stream-json 模式下通常为空, 仅崩溃时有内容)
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

    // stdout 管道先摘出来,再把 child 本体登记进全局回收池:App 退出钩子 kill_all
    // 能收割它,不再是「关 App 后继续写知识库的孤儿」(违反子进程整树回收铁律)。
    let so_pipe = child.stdout.take();
    polaris_runtime::procs::CHILDREN.insert(run_id_thread.clone(), child);
    if polaris_runtime::procs::CHILDREN.take_cancel(&run_id_thread) {
        let _ = backend.cancel(&run_id_thread);
    }
    // 硬顶看门狗:claude 挂死时本线程会永远卡在 stdout 读循环,_kb_task 维护锁
    // 随之永不释放 → 所有 KB 维护任务(enrich/dedup/compile)被锁死。超时杀树后
    // stdout EOF,读循环自然解阻塞。默认 60 分钟,POLARIS_KB_COMPILE_CAP_SECS 可调。

    let (finished, deadline) = std::sync::mpsc::channel::<()>();
    let backend_timer = backend.clone();
    let rid = run_id_thread.clone();
    std::thread::spawn(move || {
        if matches!(deadline.recv_timeout(std::time::Duration::from_secs(cap_secs)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)) {
            let _ = backend_timer.cancel(&rid);
        }
    });


    // stdout: 解析 stream-json, 把工具调用 / 写页面 / 文本翻成进度
    let mut pages: Vec<String> = Vec::new();
    if let Some(so) = so_pipe {
        emit_compile(
            &app,
            &run_id_thread,
            "phase",
            Some("读取资料、抽取实体与概念…".into()),
        );
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
            if v.get("type").and_then(|x| x.as_str()) != Some("assistant") {
                // result 事件的错误子类型 → 透传
                if v.get("type").and_then(|x| x.as_str()) == Some("result") {
                    if let Some(st) = v.get("subtype").and_then(|x| x.as_str()) {
                        if st.starts_with("error") {
                            let msg = v
                                .get("result")
                                .and_then(|x| x.as_str())
                                .unwrap_or("(unknown)")
                                .to_string();
                            emit_compile(
                                &app,
                                &run_id_thread,
                                "error",
                                Some(format!("[{st}] {msg}")),
                            );
                        }
                    }
                }
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
                        if matches!(name, "Write" | "Edit" | "MultiEdit") {
                            if let Some(fp) = block
                                .get("input")
                                .and_then(|i| i.get("file_path"))
                                .and_then(|x| x.as_str())
                            {
                                let norm = fp.replace('\\', "/");
                                let short =
                                    norm.rsplit('/').next().unwrap_or(&norm).to_string();
                                if !pages.contains(&norm) {
                                    pages.push(norm);
                                }
                                emit_compile(
                                    &app,
                                    &run_id_thread,
                                    "page",
                                    Some(format!("写入 {short}")),
                                );
                            }
                        } else {
                            emit_compile(&app, &run_id_thread, "tool", Some(name.to_string()));
                        }
                    }
                    Some("text") => {
                        if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                            let t = t.trim();
                            if !t.is_empty() {
                                emit_compile(
                                    &app,
                                    &run_id_thread,
                                    "delta",
                                    Some(t.to_string()),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let _ = finished.send(());
    // 从回收池摘回并收割;None = 看门狗已超时杀掉(或 App 退出时被 kill_all 收走)。
    let status = match polaris_runtime::procs::CHILDREN.remove(&run_id_thread) {
        Some(mut c) => c.wait(),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("超过硬顶时限 {cap_secs}s 被看门狗终止"),
        )),
    };
    // 编译完成 → 重扫刷新内存索引 + 图谱
    let n = kb_reindex();

    let ok = matches!(&status, Ok(s) if s.success());
    if !ok {
        let code = match &status {
            Ok(s) => format!("{:?}", s.code()),
            Err(e) => e.to_string(), // 看门狗超时/等待失败的原因直接透传
        };
        let se = stderr_buf.lock().clone();
        emit_compile(
            &app,
            &run_id_thread,
            "error",
            Some(format!(
                "claude 退出码 {code}{}",
                if se.is_empty() {
                    String::new()
                } else {
                    format!(" — {se}")
                }
            )),
        );
    }
    let msg = if ok {
        format!(
            "编译完成: 新建/更新 {} 个页面, 知识库共 {} 篇",
            pages.len(),
            n
        )
    } else {
        "编译中断 (见上方原因), 已刷新索引".into()
    };
    let _ = app.emit(
        "kb:compile",
        KbCompileEvent {
            run_id: run_id_thread.clone(),
            kind: "done".into(),
            text: Some(msg),
            doc_count: Some(n),
        },
    );
}
