use super::*;

pub(crate) fn reader_loop(stdout: ChildStdout, rx: mpsc::Receiver<TurnJob>, shared: Arc<SessionShared>) {
    read_jobs(BufReader::new(stdout).lines(), rx, shared);
}

struct ReaderGuard(Arc<SessionShared>);
impl Drop for ReaderGuard {
    fn drop(&mut self) {
        self.0.alive.store(false, Ordering::SeqCst);
        *self.0.active_req.lock() = None;
        *self.0.watchdog.lock() = None;
    }
}

fn read_jobs(mut lines: impl Iterator<Item = std::io::Result<String>>,
    rx: mpsc::Receiver<TurnJob>, shared: Arc<SessionShared>) {
    let _reader = ReaderGuard(shared.clone());
    while let Ok(job) = rx.recv() {
        let mut ts = pipeline::TurnStream::new(
            job.app.clone(),
            job.req_id.clone(),
            Some(job.conv_id.clone()),
            job.image_notice.clone(),
        );
        let mut got_result = false;
        for line in lines.by_ref() {
            let Ok(line) = line else { continue };
            if line.trim().is_empty() {
                continue;
            }
            job.watchdog.touch(); // 有产出就不算挂死(看门狗 idle 判据)
            if ts.feed_line(&line) {
                got_result = true; // result 事件 = 本轮收尾界标
                break;
            }
        }
        ts.flush();
        // 先清活跃标记再收尾: 看门狗以 active_req 清空为「本轮结束」, 之后的轮间
        // 静默不受 idle 判定管辖(常驻进程等下一条消息是常态, 不是卡死)。
        *shared.active_req.lock() = None;
        *shared.watchdog.lock() = None;
        // 轮次收尾续 TTL: last_used 只在轮次**开始**时刷新, 一轮跑得比 TTL(10min)久的
        // 长任务结束瞬间就已"过期", busy 豁免也随 active_req 清空失效 —— 任何别的对话
        // 一发消息, 惰性清扫就会把这个刚跑完、还带着服务端 prompt cache 的热进程收割掉,
        // 用户续问被迫重建(6.4s + 丢缓存)。这里补一次 touch, 给完整 TTL 窗口。
        // 锁序: 此刻不持任何锁, 单独取 POOL, 与全局 POOL→active_req 顺序不冲突。
        POOL.lock().touch(&shared.conv, now_ms());
        if !got_result {
            // EOF = 进程死亡。主动杀(取消/看门狗/退出)已各自告知过前端, 这里只对
            // 「自然猝死」补一条 error; 随后照常 finish 发 done, 前端正常收尾,
            // 下一轮 run_persistent_turn 检出 alive=false 自动重建。
            shared.alive.store(false, Ordering::SeqCst);
            if !shared.kill_reported.load(Ordering::SeqCst) {
                let tail = shared.stderr_tail.lock().clone();
                pipeline::emit_event(
                    &job.app,
                    ChatStreamEvent {
                        call_id: None, parent_call_id: None,
                        req_id: job.req_id.clone(),
                        kind: "error".into(),
                        text: Some(format!(
                            "claude 常驻进程中途退出, 本轮中断(下一条消息将自动重建)\n--- stderr ---\n{}",
                            if tail.is_empty() { "(stderr 为空)" } else { &tail }
                        )),
                        tool: None,
                        conversation_id: Some(job.conv_id.clone()),
                    },
                );
            }
        }
        ts.finish(&job.art_dir, &job.art_before);
        if !got_result {
            break; // stdout 已到 EOF, reader 使命结束; 池条目由下轮/收割路径清理
        }
    }
}

/// 会话级 stderr reader: 行为对齐旧路径(每行 emit error 事件 + touch 看门狗),
/// 只是错误要挂到「当前活跃轮」上; 轮间的 stderr 没有归属 req, 只进尾巴缓冲。
pub(crate) fn stderr_loop(stderr: ChildStderr, shared: Arc<SessionShared>, app: AppHandle) {
    const MAX_TAIL: usize = 64 * 1024;
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(wd) = shared.watchdog.lock().clone() {
            wd.touch();
        }
        {
            let mut tail = shared.stderr_tail.lock();
            if tail.len() < MAX_TAIL {
                tail.push_str(&line);
                tail.push('\n');
            }
        }
        if let Some(req) = shared.active_req.lock().clone() {
            pipeline::emit_event(
                &app,
                ChatStreamEvent {
                    call_id: None, parent_call_id: None,
                    req_id: req,
                    kind: "error".into(),
                    text: Some(format!("[stderr] {line}")),
                    tool: None,
                    conversation_id: Some(shared.conv.clone()),
                },
            );
        } else {
            eprintln!("[persistent-agent conv={}] stderr: {line}", shared.conv);
        }
    }
}

/// 本轮看门狗线程: 计时核仍是 polaris-watchdog(idle+硬顶), 判据与旧路径一致
/// (含 sample_tree 深检否决权)。与旧路径的关键差异: **以 result 事件为界** ——
/// active_req 被 reader 清空即本轮结束, 线程退出; 轮间静默永不触发 idle 杀。
/// 触发 → 杀常驻进程整树并告知前端, 下轮自动重建。
pub(crate) fn spawn_turn_watchdog(
    shared: Arc<SessionShared>,
    wd: Arc<polaris_watchdog::Watchdog>,
    app: AppHandle,
    req_id: String,
    conv_id: String,
    hard_cap: u64,
) {
    std::thread::spawn(move || {
        use polaris_watchdog::WatchdogVerdict;
        let base_tick = std::time::Duration::from_secs(5);
        let mut tick = base_tick;
        let mut last_cpu: Option<u64> = None;
        loop {
            std::thread::sleep(tick);
            // 本轮已收尾(result 到达)或进程已亡 → 看门狗使命结束。
            if !shared.alive.load(Ordering::SeqCst)
                || shared.active_req.lock().as_deref() != Some(req_id.as_str())
            {
                break;
            }
            let verdict = wd.check();
            let over_cap = matches!(verdict, Some(WatchdogVerdict::HardTimeout { .. }));
            let idle = wd.idle();
            match verdict {
                None => {
                    last_cpu = None;
                    tick = base_tick;
                    continue;
                }
                Some(WatchdogVerdict::IdleKilled { idle_secs }) => {
                    // 深检否决权(同旧路径): 有活子孙或 CPU 在推进 = 静默但在干活, 不杀。
                    if let Some(s) = pipeline::sample_tree(shared.pid) {
                        let cpu_advancing = last_cpu.is_none_or(|prev| s.cpu > prev);
                        last_cpu = Some(s.cpu);
                        if s.descendants > 0 || cpu_advancing {
                            tick = std::time::Duration::from_secs(30);
                            continue;
                        }
                        eprintln!(
                            "[persistent-watchdog] req={req_id} 空闲 {idle_secs}s 且进程树静止, 判挂死回收常驻进程"
                        );
                    } else {
                        eprintln!(
                            "[persistent-watchdog] req={req_id} 空闲 {idle_secs}s, 深检不可用, 回收常驻进程"
                        );
                    }
                }
                Some(WatchdogVerdict::HardTimeout { elapsed_secs }) => {
                    eprintln!(
                        "[persistent-watchdog] req={req_id} 本轮总时长 {elapsed_secs}s 超硬顶 {hard_cap}s, 回收常驻进程"
                    );
                }
            }
            // 再确认仍是本轮活跃进程才杀(防收尾/取消竞态), 摘出后锁外收割。
            let victim = {
                let mut g = POOL.lock();
                let same = g
                    .get_mut(&conv_id)
                    .is_some_and(|(_, sp)| Arc::ptr_eq(&sp.shared, &shared));
                if same && shared.active_req.lock().as_deref() == Some(req_id.as_str()) {
                    g.remove(&conv_id)
                } else {
                    None
                }
            };
            if let Some(s) = victim {
                // 先杀干净再告知(同旧路径缘由: 防旧进程缓冲尾巴混进 error 之后的新气泡);
                // reap 已置 kill_reported → reader 收 EOF 只发 done 静默收尾。
                reap(s, true);
                let reason = if over_cap {
                    format!("任务超硬顶时限: 本轮总时长超过 {hard_cap}s, 已被看门狗强制终止(常驻进程将在下一条消息自动重建)")
                } else {
                    format!(
                        "空闲超时, 进程疑似卡死已终止: claude 进程连续 {}s 无任何输出且进程树静止(下一条消息自动重建)",
                        idle.as_secs()
                    )
                };
                pipeline::emit_event(
                    &app,
                    ChatStreamEvent {
                        call_id: None, parent_call_id: None,
                        req_id: req_id.clone(),
                        kind: "error".into(),
                        text: Some(reason),
                        tool: None,
                        conversation_id: Some(conv_id.clone()),
                    },
                );
            }
            break;
        }
    });
}

// ───────────────────────── 单测 (纯记账, 时钟注入) ─────────────────────────

#[cfg(all(test, not(feature = "desktop")))]
#[path = "reader_tests.rs"]
mod tests;

