use super::*;

pub fn prewarm(
    app: AppHandle,
    conv_id: String,
    perm: String,
    with_task: bool,
    work_full: bool,
    provider_id: Option<String>,
) {
    if !persistent_enabled() {
        return;
    }
    // 产物目录与真发送同源(artifacts_dir 是纯 conv_id 映射): art_dir 进指纹的
    // add_dirs, 取值路径必须一致, 否则预热指纹永远对不上。
    let art_dir = crate::chat::artifacts::artifacts_dir(Some(&conv_id));
    let _ = std::fs::create_dir_all(&art_dir);
    // 工作目录从 conv → project 反查(与真发送 chat_send_pipeline 同源), 必须一致否则预热指纹
    // 永远对不上、白起进程。这里内部解析而不改 prewarm 签名, 免动 chat_prewarm 调用方。
    let work_dir = crate::conv::project_id_of_conversation(&conv_id)
        .and_then(|pid| crate::conv::project_work_dir(&pid));
    let p = TurnParams {
        app,
        req_id: String::new(), // 预热没有"轮", 不发消息 → 无 req_id/plan/快照
        conv_id: conv_id.clone(),
        plan: PromptPlan::new(),
        perm,
        art_dir,
        art_before: HashMap::new(),
        image_notice: None,
        with_task,
        work_full,
        provider_id,
        creative: false,
        work_dir,
    };
    let fp = fingerprint_of(&p);

    // ── 检出: 已有活进程且指纹一致 → no-op(顺手 TTL 收割; 锁内只摘, 杀在锁外) ──
    let mut to_reap: Vec<SessionProc> = Vec::new();
    let hit = {
        let mut g = POOL.lock();
        to_reap.extend(g.sweep_expired(now_ms(), session_busy));
        to_reap.extend(g.trim_over_cap(&conv_id, session_busy));
        match g.get_mut(&conv_id) {
            Some((cur_fp, sp)) if *cur_fp == fp && sp.shared.alive.load(Ordering::SeqCst) => true,
            Some(_) => {
                // 指纹变了或进程已死: 摘出去重建 —— 与 run_persistent_turn 同一套规则。
                to_reap.extend(g.remove(&conv_id));
                false
            }
            None => false,
        }
    };
    for s in to_reap {
        reap(s, true);
    }
    if hit {
        return;
    }

    // ── spawn(锁外, 数秒) + 入池 ──
    let proc = match spawn_session(&p) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("[persistent-prewarm conv={conv_id}] 预热 spawn 失败(忽略): {e}");
            return;
        }
    };
    // spawn 期间用户可能已真发消息、run_persistent_turn 抢先建好了会话 —— 此时绝不能
    // 用预热进程顶掉现役者(它可能正跑着一轮), 尊重现役、丢弃预热品。
    let (discard, evicted) = {
        let mut g = POOL.lock();
        let occupied = g
            .get_mut(&conv_id)
            .is_some_and(|(_, sp)| sp.shared.alive.load(Ordering::SeqCst));
        if occupied {
            (Some(proc), Vec::new())
        } else {
            (None, g.insert(conv_id.clone(), fp, proc, now_ms(), session_busy))
        }
    };
    if let Some(s) = discard {
        reap(s, true);
    }
    for s in evicted {
        reap(s, true);
    }
}

/// 常驻路径主入口: 检出/重建会话进程 → 写本轮消息 → 挂 job + 看门狗。
/// 进程死亡/写失败 → 透明重建并把本条按「首条」处理(不向用户报错), 最多重试 2 次。
pub(crate) fn run_persistent_turn(p: TurnParams) -> Result<(), String> {
    let fp = fingerprint_of(&p);

    let mut last_err = String::from("(未知)");
    for _attempt in 0..2 {
        // ── 1. 池内检出: TTL 惰性收割 + 指纹/死活校验(锁内只摘, 杀在锁外) ──
        let mut to_reap: Vec<SessionProc> = Vec::new();
        let mut reusable = false;
        {
            let mut g = POOL.lock();
            to_reap.extend(g.sweep_expired(now_ms(), session_busy));
            to_reap.extend(g.trim_over_cap(&p.conv_id, session_busy));
            if let Some((cur_fp, sp)) = g.get_mut(&p.conv_id) {
                if *cur_fp == fp && sp.shared.alive.load(Ordering::SeqCst) {
                    reusable = true;
                    // 就在锁内把本轮登记上(见下方 insert 处的大注释):否则从这里放锁
                    // 到第 5 步真正注册之间,这个会话在 session_busy 眼里是「空闲」,
                    // 会被另一条并发对话的 trim_over_cap/insert 当 LRU 牺牲品逐掉。
                    *sp.shared.active_req.lock() = Some(p.req_id.clone());
                } else {
                    // 指纹变了(权限/供应商/模型/创作切换)或进程已死 → 杀旧重建
                    to_reap.extend(g.remove(&p.conv_id));
                }
            }
        }
        for s in to_reap {
            reap(s, true);
        }

        // ── 2. 需要则重спавн(锁外 spawn, 插入时 LRU 逐出的旧会话也在锁外收割) ──
        if !reusable {
            let proc = match spawn_session(&p) {
                Ok(x) => x,
                Err(e) => {
                    last_err = e;
                    continue;
                }
            };
            // ⚠ 本轮登记必须**先于入池**。
            //
            // session_busy = alive && active_req.is_some(),而新 spawn 出来的会话
            // active_req 还是 None —— 旧写法要等到第 5 步才登记。于是「入池 → 检出」
            // 这段窗口里,它在所有逐出谓词眼里都是一个「空闲会话」:
            //   · 另一条并发对话第 1 步的 trim_over_cap 会挑中它(它 last_used 最老);
            //   · 另一条并发对话第 2 步的 insert 也会挑中它(insert 只护自己那一个新条目)。
            // MAX_SESSIONS=2,所以只要同时新建 3 条以上对话,落后的那些必被抢先者收割,
            // 到第 3 步就报「会话条目在检出后消失(并发收割)」——2 次重试打光后用户直接
            // 看到发送失败。2026-07-29 压测实测:4 路并发新建对话 3/4 失败,9 路 8/9 失败。
            //
            // 登记提前后它一入池就是「忙碌」,谁也逐不走;所有失败路径(stdin 写失败 /
            // reader 已退出 / 条目消失)本来就会 remove+reap,reap 置 alive=false,
            // 而 session_busy 要求 alive —— 不会留下逐不掉的僵尸条目。
            *proc.shared.active_req.lock() = Some(p.req_id.clone());
            let evicted =
                POOL.lock()
                    .insert(p.conv_id.clone(), fp.clone(), proc, now_ms(), session_busy);
            for s in evicted {
                reap(s, true);
            }
        }

        // ── 3. 取会话句柄 + 决定首条/续轮文本 ──
        let (stdin, job_tx, shared, sent, is_first) = {
            let mut g = POOL.lock();
            let Some((_, sp)) = g.get_mut(&p.conv_id) else {
                last_err = "会话条目在检出后消失(并发收割)".into();
                continue;
            };
            (
                sp.stdin.clone(),
                sp.job_tx.clone(),
                sp.shared.clone(),
                sp.sent_keys.clone(),
                !sp.first_done,
            )
        };
        let text = if is_first {
            p.plan.full_text()
        } else {
            p.plan.continuation_text(&sent)
        };
        // stream-json 输入协议: 每轮一行 user 消息。serde 负责转义, 天然单行。
        let line = serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": [{ "type": "text", "text": text }] }
        })
        .to_string();

        // ── 4. 写 stdin(锁外; 失败 = 进程死了 → 杀掉重来, 本条按首条) ──
        let write_ok = {
            let mut si = stdin.lock();
            si.write_all(line.as_bytes())
                .and_then(|_| si.write_all(b"\n"))
                .and_then(|_| si.flush())
        };
        if let Err(e) = write_ok {
            last_err = format!("常驻进程 stdin 写失败: {e}");
            if let Some(s) = POOL.lock().remove(&p.conv_id) {
                reap(s, true);
            }
            continue;
        }
        // 写成功才记账: sent_keys 并入本轮门控 key、置 first_done、touch LRU。
        {
            let mut g = POOL.lock();
            if let Some((_, sp)) = g.get_mut(&p.conv_id) {
                for k in p.plan.keys() {
                    sp.sent_keys.insert(k);
                }
                sp.first_done = true;
            }
            g.touch(&p.conv_id, now_ms());
        }

        // ── 5. 注册本轮 + 派发 job + 挂看门狗 ──
        let wd = Arc::new(polaris_watchdog::Watchdog::new(
            polaris_watchdog::WatchdogConfig {
                idle_secs: pipeline::watchdog_config().0,
                hard_cap_secs: pipeline::watchdog_config().1,
            },
        ));
        *shared.active_req.lock() = Some(p.req_id.clone());
        *shared.watchdog.lock() = Some(wd.clone());
        let job = TurnJob {
            completion: super::super::completion::TurnGuard::new(&p.req_id, Some(&p.conv_id)),
            req_id: p.req_id.clone(),
            conv_id: p.conv_id.clone(),
            app: p.app.clone(),
            art_dir: p.art_dir.clone(),
            art_before: p.art_before.clone(),
            image_notice: p.image_notice.clone(),
            watchdog: wd.clone(),
        };
        if let Err(mut error) = job_tx.send(job) {
            error.0.completion.disarm();
            // reader 线程已死(进程 EOF) → 同 stdin 失败: 杀掉重来
            *shared.active_req.lock() = None;
            last_err = "常驻进程 reader 已退出".into();
            if let Some(s) = POOL.lock().remove(&p.conv_id) {
                reap(s, true);
            }
            continue;
        }
        let (idle_secs, hard_cap) = pipeline::watchdog_config();
        if idle_secs > 0 || hard_cap > 0 {
            spawn_turn_watchdog(
                shared.clone(),
                wd,
                p.app.clone(),
                p.req_id.clone(),
                p.conv_id.clone(),
                hard_cap,
            );
        }

        // 停止在「注册前窄窗口」到达的兜底(同旧路径 spawn 后补查): 有标记即刻杀会话,
        // reader 收 EOF 静默收尾发 done。
        if CHILDREN.take_cancel(&p.req_id) {
            cancel_by_req(&p.req_id);
        }
        return Ok(());
    }
    Err(format!("常驻 claude 进程不可用(已重试): {last_err}"))
}

// ───────────────────────── spawn + 常驻线程 ─────────────────────────

/// 起一个常驻会话进程: spawn 参数全套沿用旧路径(process_group/no_window/
/// harden_child_env/scope_child_claude_by_id 都在 spawn_on_host 里), 只多
/// `--input-format stream-json` 让 stdin 变成逐轮消息通道而非一次性 prompt。
fn spawn_session(p: &TurnParams) -> Result<SessionProc, String> {
    let mut child = pipeline::spawn_on_host(
        "",
        &p.perm,
        &p.art_dir,
        p.with_task,
        p.work_full,
        p.provider_id.as_deref(),
        true, // persistent: 加 --input-format stream-json
        p.work_dir.as_deref().map(std::path::Path::new),
    )?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "常驻 claude 子进程没有 stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "常驻 claude 子进程没有 stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "常驻 claude 子进程没有 stderr".to_string())?;
    let shared = Arc::new(SessionShared {
        conv: p.conv_id.clone(),
        pid: child.id(),
        alive: AtomicBool::new(true),
        kill_reported: AtomicBool::new(false),
        active_req: Mutex::new(None),
        watchdog: Mutex::new(None),
        stderr_tail: Mutex::new(String::new()),
    });
    let (tx, rx) = mpsc::channel::<TurnJob>();
    {
        let sh = shared.clone();
        std::thread::spawn(move || reader_loop(stdout, rx, sh));
    }
    {
        let sh = shared.clone();
        let app = p.app.clone();
        std::thread::spawn(move || stderr_loop(stderr, sh, app));
    }
    Ok(SessionProc {
        child: Some(child),
        stdin: Arc::new(Mutex::new(stdin)),
        job_tx: tx,
        shared,
        sent_keys: HashSet::new(),
        first_done: false,
    })
}
