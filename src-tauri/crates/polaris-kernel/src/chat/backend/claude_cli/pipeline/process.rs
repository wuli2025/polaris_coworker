use super::*;

pub(crate) fn run_process(
    app: &AppHandle, req_id: &str, args: ChatSendArgs,
    art_dir: std::path::PathBuf,
    art_before: std::collections::HashMap<std::path::PathBuf, SystemTime>,
    project_work_dir: Option<String>, plan: session_pool::PromptPlan,
    image_notice: Option<String>, work_full: bool, creative: bool, full_prompt: String,
) -> Result<(), String> {
    let perm = args.permission_mode.cli_value();
    let conv_id_opt = args.conversation_id.clone();
    // ── 常驻 agent 路径(默认开, POLARIS_PERSISTENT_AGENT=0 回落) ──
    // claude CLI 自举 ~6.4s(bun 解包)是每消息 spawn 的硬成本; 常驻进程 + stream-json
    // 输入把续轮首事件降到 1.4-2.6s。仅宿主机 + 有 conv_id 的对话走此路(沙箱路径不动;
    // 无 conv_id 没有池 key, 也没有续轮语义)。
    if session_pool::persistent_enabled() && !args.use_sandbox {
        if let Some(cid) = conv_id_opt.clone() {
            return session_pool::run_persistent_turn(session_pool::TurnParams {
                app: app.clone(),
                req_id: req_id.to_string(),
                conv_id: cid,
                plan,
                perm: perm.to_string(),
                art_dir,
                art_before,
                image_notice,
                with_task: args.dynamic_workflow,
                work_full,
                provider_id: args.provider_id.clone(),
                creative,
                work_dir: project_work_dir.clone(),
            });
        }
    }

    // ── 旧路径(每消息 spawn): 逻辑保持不变, prompt 取分段计划的全量拼接 ──
    let mut final_prompt = full_prompt;

    // 默认走宿主机执行（沙箱可选，但默认关闭）；动态编排时放行 Task 子代理；
    // work_full 决定快速模式是否禁用冗余工具(disallowedTools)、是否传按模式的 --model。
    let mut child = spawn_on_host(
        &final_prompt,
        perm,
        &art_dir,
        args.dynamic_workflow,
        work_full,
        args.provider_id.as_deref(),
        false, // 一次性进程: stdin 只当 prompt 通道, 不进 stream-json 输入模式
        project_work_dir.as_deref().map(Path::new),
    )?;

    // prompt 经 stdin 喂给 claude (而非命令行参数): 大 prompt 不会撞 Windows 命令行
    // 长度上限, 也不会因 prompt 以 `-` 开头被当成 flag。spawn 后立刻写 + drop, claude 读到 EOF 就开始处理。
    // stdin 写放独立线程: 大 prompt 超过 OS 管道缓冲(~64KB)且 claude 尚未开始读时,
    // write_all 会阻塞 —— 放后台线程就不会卡住本 async 命令的执行线程(影响其它并发命令)。
    // 写完线程结束时 drop(stdin) 关管道 → claude 读到 EOF 开工。失败不致命(claude 有 fallback)。
    if let Some(mut stdin) = child.stdin.take() {
        let payload = std::mem::take(&mut final_prompt);
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(payload.as_bytes());
        });
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "claude 子进程没有 stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "claude 子进程没有 stderr".to_string())?;

    CHILDREN.insert(req_id.to_string(), child);

    // 看门狗(容器/服务端稳健性): 个别 prompt 会让 claude 触发子代理(`claude --print`,
    // 容器内其 cwd 落在 `/`)对文件系统做无界扫描而长时间不返回 —— 既拖死本轮, 又占住
    // OAuth 订阅的并发槽拖垮后续消息。判据分两层:
    // ① **计时判定核**已抽到 polaris-watchdog crate(空闲 idle + 绝对硬顶 hard cap,
    //    纯逻辑可独立测试): stdout/stderr 每产出一行就 touch → 持续有输出的长任务
    //    (批量 PPT/长脚本)idle 永不触发; 阈值仍由原有 env 控制(与旧行为完全融合,
    //    不叠两套): POLARIS_CHAT_TIMEOUT_SECS 桌面 1800s / 容器 900s, 0=关;
    //    POLARIS_CHAT_HARD_CAP_SECS 默认 86400s(24h, 只兜真失控, 7h 级长任务不受扰)。
    // ② idle 判定后 kernel 保留**深检进程树的否决权**: 还有活的子孙进程(claude 正在跑
    //    Bash 工具里的构建/ffmpeg/下载等, 工具执行期整段零输出是常态), 或整树 CPU 时间
    //    仍在推进(claude 本体在算) —— 都算「静默但在干活」, 不杀, 转入 30s 低频复查;
    //    只有**连续两次采样都完全静止**(零子孙 + CPU 零推进, 真挂死/网络吊死的特征)
    //    才杀整树。深检失败(平台探测不可用)退回空闲即杀, 保住容器自愈。深检依赖平台
    //    进程探测, 不属于纯逻辑, 故留在 kernel 而不进 watchdog crate。
    // 杀前 Child 已从 CHILDREN 摘出, 回收原因由看门狗自己 emit error 告知前端;
    // 杀掉后 claude stdout 随之关闭 → 下面 reader 线程照常收尾 emit done, 系统自愈。
    // 桌面此前默认 0=不启用 → 挂死的 claude 子进程(及其 cwd=/ 子代理)永不超时,
    // 一年长跑里偶发网络故障累积出几十个吊死进程+阻塞线程,耗尽句柄/FD。默认常开,
    // 仍可经 POLARIS_CHAT_TIMEOUT_SECS 覆写(设 0 显式关闭)。阈值读取抽到
    // watchdog_config(常驻路径复用同一套)。
    let (watchdog_timeout, hard_cap) = watchdog_config();
    let watchdog = Arc::new(polaris_watchdog::Watchdog::new(
        polaris_watchdog::WatchdogConfig {
            idle_secs: watchdog_timeout,
            hard_cap_secs: hard_cap,
        },
    ));
    if watchdog_timeout > 0 || hard_cap > 0 {
        use polaris_watchdog::WatchdogVerdict;
        let wd_req = req_id.to_string();
        let wd = watchdog.clone();
        let wd_app = app.clone();
        let wd_conv = conv_id_opt.clone();
        std::thread::spawn(move || {
            // 检查节拍: 常态每 5s 看一次空闲; 进入「静默但在干活」延期区后放缓到 30s,
            // 深检(进程快照)只在延期区跑, 常态零开销。轻量线程 + sleep 轮询,
            // 不引入任何异步运行时依赖(与旧实现一致)。
            let base_tick = if watchdog_timeout > 0 {
                std::cmp::min(
                    std::time::Duration::from_secs(watchdog_timeout),
                    std::time::Duration::from_secs(5),
                )
            } else {
                std::time::Duration::from_secs(5)
            };
            let mut tick = base_tick;
            let mut last_cpu: Option<u64> = None;
            loop {
                std::thread::sleep(tick);
                // 先做纯计时判定(不与 CHILDREN 锁同时持有, 避免锁序问题), 再持锁取 pid:
                // 取到 Some 才证明仍是本 req 的活进程; 取到 None = 已正常结束被 stdout
                // 线程 remove → 退出看门狗。深检可能耗几十 ms, 不在锁内做。
                let verdict = wd.check();
                let pid = {
                    let g = CHILDREN.lock();
                    let Some(c) = g.get(&wd_req) else { break };
                    c.id()
                };
                let over_cap = matches!(verdict, Some(WatchdogVerdict::HardTimeout { .. }));
                let idle = wd.idle();
                match verdict {
                    None => {
                        // 有输出在推进, 回到常态节拍并清掉 CPU 基线。
                        last_cpu = None;
                        tick = base_tick;
                        continue;
                    }
                    Some(WatchdogVerdict::IdleKilled { idle_secs }) => {
                        // idle 判定进入嫌疑区 → 深检进程树行使否决权(见文件头注释②)。
                        if let Some(s) = sample_tree(pid) {
                            // 首次越阈只建 CPU 基线不杀(cpu_advancing 视为 true), 下次采样再比对。
                            let cpu_advancing = last_cpu.is_none_or(|prev| s.cpu > prev);
                            last_cpu = Some(s.cpu);
                            if s.descendants > 0 || cpu_advancing {
                                tick = std::time::Duration::from_secs(30);
                                continue; // 静默但在干活: 不杀, 低频续看
                            }
                            eprintln!(
                                "[chat-watchdog] req={wd_req} 空闲 {idle_secs}s 且进程树静止(0 子孙/CPU 无推进), 判挂死回收"
                            );
                        } else {
                            eprintln!(
                                "[chat-watchdog] req={wd_req} 空闲 {idle_secs}s, 进程树深检不可用, 按旧策略回收"
                            );
                        }
                    }
                    Some(WatchdogVerdict::HardTimeout { elapsed_secs }) => {
                        eprintln!(
                            "[chat-watchdog] req={wd_req} 总时长 {elapsed_secs}s 超硬顶 {hard_cap}s, 无条件回收"
                        );
                    }
                }
                // 重新确认仍是本 req 的同一进程再杀(防深检窗口内正常结束 + PID 复用误杀)。
                // 锁内只做「确认 + 摘出 Child」; kill_tree(Windows taskkill 同步等待常见
                // 数百 ms)与 kill/wait 全在锁外 —— CHILDREN 是全局锁, 锁内阻塞会把**所有
                // 对话**的 insert/remove(每次发送与收尾)一起钉住(范式同 chat_cancel /
                // ChildRegistry::kill)。摘出而非借用: 持有 Child 所有权 = reader 线程
                // 无法先 remove+reap → pid 在 kill 前绝不会被复用。
                let victim = {
                    let mut g = CHILDREN.lock();
                    let same_proc = g.get(&wd_req).is_some_and(|c| c.id() == pid);
                    if same_proc {
                        g.remove(&wd_req)
                    } else {
                        None
                    }
                };
                if let Some(mut c) = victim {
                    // 深检与摘出之间进程可能恰好正常结束:先 try_wait, 已退出就只 reap,
                    // 不把一次成功的请求误报成「被看门狗回收」。
                    if matches!(c.try_wait(), Ok(Some(_))) {
                        let _ = c.wait();
                    } else {
                        // 先杀干净**再**告知前端:反过来的话, 管道里已缓冲的 delta 会在
                        // error 之后继续上屏, 而移动端把 error 当终态立即放行下一问,
                        // 旧进程的尾巴会混进新一轮气泡。
                        kill_tree(pid); // 杀进程组: 一并带走 cwd=/ 的子代理
                        let _ = c.kill(); // 兜底杀本体 (taskkill /T 通常已带走它)
                        let _ = c.wait(); // reap, 防 Unix 僵尸进程泄漏
                        // Child 已摘出, reader 线程收尾时查不到 → 不再发「异常退出」error
                        // (同 chat_cancel 语义)。回收原因由这里告知前端, 不至于无声中断。
                        let reason = if over_cap {
                            format!("任务超硬顶时限: 本轮总时长超过 {hard_cap}s, 已被看门狗强制终止")
                        } else {
                            format!(
                                "空闲超时, 进程疑似卡死已终止: claude 进程连续 {}s 无任何输出且进程树静止",
                                idle.as_secs()
                            )
                        };
                        emit_event(
                            &wd_app,
                            ChatStreamEvent {
                                call_id: None, parent_call_id: None,
                                req_id: wd_req.clone(),
                                kind: "error".into(),
                                text: Some(reason),
                                tool: None,
                                conversation_id: wd_conv.clone(),
                            },
                        );
                    }
                }
                break;
            }
        });
    }

    // stderr 读线程: 任何 stderr 行都 emit 为 error 事件; 累积起来给 wait 用
    let app_err = app.clone();
    let req_err = req_id.to_string();
    let conv_id_err = conv_id_opt.clone();
    let stderr_buf = Arc::new(Mutex::new(String::new()));
    let stderr_buf_clone = stderr_buf.clone();
    let act_err = watchdog.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.trim().is_empty() {
                continue;
            }
            act_err.touch(); // 刷新活动: 有产出就不算挂死
            {
                // 单次加锁 + 封顶: 异常时 stderr 也可能狂刷, 不让它无界累积。
                let mut buf = stderr_buf_clone.lock();
                if buf.len() < MAX_STDERR_BYTES {
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
            emit_event(
                &app_err,
                ChatStreamEvent {
                    call_id: None, parent_call_id: None,
                    req_id: req_err.clone(),
                    kind: "error".into(),
                    text: Some(format!("[stderr] {}", line)),
                    tool: None,
                    conversation_id: conv_id_err.clone(),
                },
            );
        }
    });

    // stdout 读线程: stream-json -> 事件; 累积 assistant 文本 + 产物路径
    let app_out = app.clone();
    let req_out = req_id.to_string();
    let conv_id_thread = conv_id_opt.clone();
    let stderr_buf_for_done = stderr_buf.clone();
    let art_dir_thread = art_dir.clone();
    let act_out = watchdog.clone();
    let completion = super::super::completion::TurnGuard::new(req_id, conv_id_opt.as_deref());
    std::thread::spawn(move || {
        let _completion = completion;
        let reader = BufReader::new(stdout);
        // 单轮事件流状态机(解析/合批/记账/产物)抽到 TurnStream: 与常驻 agent 路径
        // (session_pool::reader_loop)共用同一实现, 事件契约只此一份。
        let mut ts = TurnStream::new(
            app_out.clone(),
            req_out.clone(),
            conv_id_thread.clone(),
            image_notice,
        );
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.trim().is_empty() {
                continue;
            }
            act_out.touch(); // 刷新活动: 流式产出即视为推进, 防误杀
            let _ = ts.feed_line(&line); // 一次性进程以 EOF 收尾, result 界标只有常驻路径用
        }
        // 流结束必 flush: 缓冲里最后一撮 delta 要先于 error/artifact/done 事件落地。
        ts.flush();

        // 等子进程退出, 检查 exit code (不能持锁 wait, 否则 chat_cancel 死锁)
        let child_opt = CHILDREN.remove(&req_out);
        let exit_msg: Option<String> = if let Some(mut child) = child_opt {
            // 有界等待: 卡死的孙进程 (占着管道不退) 绝不能把这个读线程永久钉住,
            // 否则一年里每次卡死都泄漏一个 ~2MB 栈的线程 → 终将 OOM。
            // 非阻塞 try_wait 轮询 + 硬死线, 到点强杀回收 (关管道) 再走异常退出路径。
            // 注意: 这里只补一道兜底, stdout/stderr 的读取与 emit、看门狗、事件载荷全不变。
            let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(900);
            let waited: std::io::Result<Option<std::process::ExitStatus>> = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Ok(Some(status)),
                    Ok(None) => {
                        if std::time::Instant::now() >= wait_deadline {
                            let _ = child.kill(); // 强杀回收: 关掉管道, 不让本线程泄漏
                            let _ = child.wait(); // 杀后做一次简短最终 reap
                                                  // 拿不到真实状态就当超时异常 (走下方 None 分支 → 同款错误事件)
                            break Ok(child.try_wait().ok().flatten());
                        }
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                    Err(e) => break Err(e),
                }
            };
            match waited {
                Ok(Some(status)) => {
                    if !status.success() {
                        let stderr_txt = stderr_buf_for_done.lock().clone();
                        Some(format!(
                            "claude 进程异常退出 (exit code={:?})\n--- stderr ---\n{}",
                            status.code(),
                            if stderr_txt.is_empty() {
                                "(stderr 为空)".to_string()
                            } else {
                                stderr_txt
                            }
                        ))
                    } else {
                        None
                    }
                }
                Ok(None) => {
                    let stderr_txt = stderr_buf_for_done.lock().clone();
                    Some(format!(
                        "claude 进程等待超时, 已强制回收\n--- stderr ---\n{}",
                        if stderr_txt.is_empty() {
                            "(stderr 为空)".to_string()
                        } else {
                            stderr_txt
                        }
                    ))
                }
                Err(e) => Some(format!("等待 claude 进程失败: {}", e)),
            }
        } else {
            None
        };

        if let Some(msg) = exit_msg {
            emit_event(
                &app_out,
                ChatStreamEvent {
                    call_id: None, parent_call_id: None,
                    req_id: req_out.clone(),
                    kind: "error".into(),
                    text: Some(msg),
                    tool: None,
                    conversation_id: conv_id_thread.clone(),
                },
            );
        }

        // 收尾(产物快照 diff → 落库 → done → 清取消标记)与常驻路径共用 TurnStream::finish。
        ts.finish(&art_dir_thread, &art_before);
    });

    // stop 在「spawn 后、child 注册进 CHILDREN 前」的窄窗口内到达的兜底: 那一刻
    // chat_cancel 找不到 child 只能打标记, 这里(reader 线程已挂接后)补一次检查 ——
    // 有标记就按正常取消路径杀掉; stdout 随之关闭, reader 线程照常发 done 收尾,
    // 且 child 已先从 CHILDREN 摘除 → 不会发「异常退出」error(与 chat_cancel 同款语义)。
    if CHILDREN.take_cancel(req_id) {
        if let Some(mut c) = CHILDREN.remove(req_id) {
            kill_tree(c.id());
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    Ok(())
}
