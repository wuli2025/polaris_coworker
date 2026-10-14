use super::*;

fn emit(app: &AppHandle, ev: EnvStreamEvent) {
    let _ = app.emit("env:stream", ev);
}

/// 起子进程, 双线程读 stdout/stderr → `env:stream` 日志; 退出后(可选)修 PATH, 再发 done。
pub(crate) fn stream_install(
    app: AppHandle,
    req_id: String,
    mut cmd: Command,
    fix_path_after: bool,
    label: &str,
) {
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            emit(
                &app,
                EnvStreamEvent {
                    req_id,
                    kind: "done".into(),
                    line: None,
                    ok: Some(false),
                    message: Some(format!("启动安装进程失败: {e} (系统 shell 是否可用?)")),
                },
            );
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    CHILDREN.insert(req_id.clone(), child);

    // stderr 线程
    if let Some(stderr) = stderr {
        let app_e = app.clone();
        let req_e = req_id.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let Ok(line) = line else { continue };
                if line.trim().is_empty() {
                    continue;
                }
                emit(
                    &app_e,
                    EnvStreamEvent {
                        req_id: req_e.clone(),
                        kind: "log".into(),
                        line: Some(line),
                        ok: None,
                        message: None,
                    },
                );
            }
        });
    }

    // stdout 线程 (主): 读完 → wait → 修 PATH → done
    let label = label.to_string();
    std::thread::spawn(move || {
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { continue };
                if line.trim().is_empty() {
                    continue;
                }
                emit(
                    &app,
                    EnvStreamEvent {
                        req_id: req_id.clone(),
                        kind: "log".into(),
                        line: Some(line),
                        ok: None,
                        message: None,
                    },
                );
            }
        }

        let child_opt = CHILDREN.remove(&req_id);
        let success = if let Some(mut child) = child_opt {
            child.wait().map(|s| s.success()).unwrap_or(false)
        } else {
            // 被 cancel 掉了
            emit(
                &app,
                EnvStreamEvent {
                    req_id: req_id.clone(),
                    kind: "done".into(),
                    line: None,
                    ok: Some(false),
                    message: Some("安装已取消。".into()),
                },
            );
            return;
        };

        let mut message = if success {
            format!("{label} 安装完成。")
        } else {
            format!("{label} 安装未成功 (进程非零退出)，可查看上方日志或改用其他方式重试。")
        };

        // 装完 Node: 把 node bin 目录塞进**进程** PATH, 让同一会话里紧接着的 npm/claude 安装
        // 立刻找得到 npm —— 安装器(Win MSI / mac 写 shell 配置)只为「新进程/新终端」配 PATH,
        // 本进程不刷新就会「装了 Node 还说没 npm」。两端通用 (node_dir_candidates 按平台给候选)。
        if success {
            if let Some(dir) = node_dir_candidates().into_iter().find(|p| p.exists()) {
                prepend_process_path(&dir.to_string_lossy());
            }
        }

        // 装完 claude 的路径可能变了 → 清空 chat spawn 的解析缓存, 下次重新解析
        if success {
            *CLAUDE_EXE_CACHE.lock() = None;
            // 若真身 pwsh 已就位 (本次刚装好, 或本就装了), 顺手把它的目录注入 PATH(进程+用户),
            // 让本进程 spawn 的 claude 立刻用上 —— 装完 PowerShell 7 免重启即可对话。
            if let Some(fix) = ensure_pwsh_on_path() {
                if fix.ok && fix.status == "added" {
                    message.push('\n');
                    message.push_str(&fix.message);
                }
            }
        }

        // 成功后自动修 PATH (改环境变量) —— 这是「装完即可用」的关键
        if success && fix_path_after {
            let report = env_check_sync();
            if let Some(dir) = report.claude_dir {
                let fix = ensure_dir_on_path(&dir);
                message.push('\n');
                message.push_str(&fix.message);
            }
        }

        emit(
            &app,
            EnvStreamEvent {
                req_id: req_id.clone(),
                kind: "done".into(),
                line: None,
                ok: Some(success),
                message: Some(message),
            },
        );
    });
}

