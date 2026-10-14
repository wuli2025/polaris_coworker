use super::*;

#[test]
fn whitelist_accepts_plain_command() {
    assert!(check_whitelist("git", &["status".into(), "--short".into()]).is_ok());
    assert!(check_whitelist("git.exe", &["log".into()]).is_ok());
    assert!(check_whitelist("GIT", &[]).is_ok(), "命令名大小写不敏感");
}

#[test]
fn whitelist_rejects_unlisted() -> Result<(), String> {
    let e = check_whitelist("rm", &["-rf".into(), "/".into()]).err().ok_or("expected rejection")?;
    assert!(e.contains("不在白名单"), "{e}");
    Ok(())
}

#[test]
fn whitelist_rejects_path_escape() {
    // 带路径就能指向白名单外的任意 exe，必须拒。
    assert!(check_whitelist("../../evil", &[]).is_err());
    assert!(check_whitelist("/usr/bin/rm", &[]).is_err());
    assert!(check_whitelist("C:\\Windows\\System32\\cmd.exe", &[]).is_err());
}

#[test]
fn whitelist_rejects_meta_chars_in_args() {
    // 纵深防御：即便我们不过 shell，被调程序可能自己过。
    for bad in ["a|b", "a;b", "a&b", "$(x)", "`x`", "a>b", "a\nb"] {
        assert!(
            check_whitelist("git", &["log".into(), bad.to_string()]).is_err(),
            "应拒绝元字符 {bad}"
        );
    }
}

#[test]
fn whitelist_allows_normal_flags_and_paths_in_args() {
    // 参数里的路径是正常的（只有命令名不许带路径）。
    assert!(check_whitelist("git", &["-C".into(), "D:\\repo".into(), "status".into()]).is_ok());
    assert!(check_whitelist("rg", &["--hidden".into(), "foo.*bar".into()]).is_ok());
}

// ── 端到端：走真实的 run()（真临时库 + 真起进程），验证四道闸的实际行为 ──
//
// 这些测试互设 POLARIS_COLLAB_DB，必须串行（TEST_LOCK），且不能与 db 的其它
// 测试并发。用 cargo 自己当被测命令：跑 cargo test 时它必然在 PATH 上。

fn with_tmp_db(name: &str) -> std::path::PathBuf {
    let tmp = std::env::temp_dir().join(format!("polaris-exec-test-{name}.db"));
    let _ = std::fs::remove_file(&tmp);
    std::env::set_var("POLARIS_COLLAB_DB", &tmp);
    tmp
}

fn req(cmd: &str, args: &[&str]) -> ExecRequest {
    ExecRequest {
        cmd: cmd.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        line: String::new(),
        cwd: String::new(),
        timeout_secs: Some(60),
    }
}

#[tokio::test]
async fn gate1_disabled_by_default_rejects() -> Result<(), String> {
    let _g = crate::collab::db::TEST_LOCK.lock().map_err(|e| e.to_string())?;
    let tmp = with_tmp_db("gate1");
    // 全新库 = 没有 exec_enabled 键 = 默认关。
    assert!(!policy().enabled, "远程执行必须默认关闭");
    let e = run("tester", req("cargo", &["--version"])).await.err().ok_or("expected rejection")?;
    assert!(e.contains("未开启远程执行"), "{e}");
    std::env::remove_var("POLARIS_COLLAB_DB");
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

#[tokio::test]
async fn whitelist_mode_runs_listed_and_blocks_rest() -> Result<(), String> {
    let _g = crate::collab::db::TEST_LOCK.lock().map_err(|e| e.to_string())?;
    let tmp = with_tmp_db("wl");
    set_policy(true, None)?;
    assert_eq!(policy().mode, "whitelist", "开总开关后默认必须是白名单模式");

    // 在册命令真跑起来，拿到真实退出码与 stdout。
    let r = run("tester", req("cargo", &["--version"])).await?;
    assert!(r.ok, "cargo --version 应成功: {:?}", r);
    assert_eq!(r.exit_code, Some(0));
    assert!(r.stdout.contains("cargo"), "stdout={}", r.stdout);
    assert_eq!(r.mode, "whitelist");

    // 不在册的命令：拒在闸门，不落到 spawn。
    let e = run("tester", req("rm", &["-rf", "x"])).await.err().ok_or("expected rejection")?;
    assert!(e.contains("不在白名单"), "{e}");

    // 白名单模式不接受整行 shell。
    let mut lreq = req("", &[]);
    lreq.line = "cargo --version | findstr cargo".into();
    let e = run("tester", lreq).await.err().ok_or("expected rejection")?;
    assert!(e.contains("白名单模式"), "{e}");

    std::env::remove_var("POLARIS_COLLAB_DB");
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

#[tokio::test]
async fn shell_mode_unlocks_then_expires() -> Result<(), String> {
    let _g = crate::collab::db::TEST_LOCK.lock().map_err(|e| e.to_string())?;
    let tmp = with_tmp_db("shell");
    set_policy(true, Some(30))?;
    assert_eq!(policy().mode, "shell", "解锁 30 分钟后应是 shell 模式");

    // 整行命令交给 shell，管道可用。
    let mut lreq = req("", &[]);
    lreq.line = if cfg!(windows) {
        "echo hello-shell".into()
    } else {
        "echo hello-shell | tr a-z A-Z".into()
    };
    let r = run("tester", lreq).await?;
    assert!(r.ok, "shell 整行命令应成功: {:?}", r);
    assert!(
        r.stdout.to_lowercase().contains("hello-shell"),
        "stdout={}",
        r.stdout
    );
    assert_eq!(r.mode, "shell");

    // 过期即自动落回白名单——不需要任何定时清理任务。
    crate::collab::db::meta_set(K_SHELL_UNTIL, &(now_secs() - 1).to_string())?;
    assert_eq!(policy().mode, "whitelist", "shell 授权过期后必须自动落回白名单");

    // 关总开关会顺手清掉 shell 解锁，防下次开启时继承悬空授权。
    set_policy(true, Some(30))?;
    set_policy(false, None)?;
    set_policy(true, None)?;
    assert_eq!(policy().mode, "whitelist", "关开关必须清掉 shell 解锁");

    std::env::remove_var("POLARIS_COLLAB_DB");
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

#[tokio::test]
async fn timeout_kills_runaway_command() -> Result<(), String> {
    let _g = crate::collab::db::TEST_LOCK.lock().map_err(|e| e.to_string())?;
    let tmp = with_tmp_db("timeout");
    set_policy(true, Some(30))?; // shell 模式才好造一条长命令
    let mut lreq = req("", &[]);
    lreq.line = if cfg!(windows) {
        "Start-Sleep -Seconds 30".into()
    } else {
        "sleep 30".into()
    };
    lreq.timeout_secs = Some(2);
    let started = std::time::Instant::now();
    let r = run("tester", lreq).await?;
    assert!(r.timed_out, "应被超时终止: {:?}", r);
    assert_eq!(r.exit_code, None);
    assert!(
        started.elapsed().as_secs() < 10,
        "超时后必须立即返回,实际 {:?}",
        started.elapsed()
    );
    std::env::remove_var("POLARIS_COLLAB_DB");
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

#[tokio::test]
async fn audit_records_every_attempt() -> Result<(), String> {
    let _g = crate::collab::db::TEST_LOCK.lock().map_err(|e| e.to_string())?;
    let tmp = with_tmp_db("audit");
    set_policy(true, None)?;
    let _ = run("alice", req("cargo", &["--version"])).await?;
    let _ = run("mallory", req("rm", &["-rf", "/"])).await.err().ok_or("expected rejection")?;

    let rows = crate::collab::db::audit_recent(50)?;
    assert!(
        rows.iter().any(|r| r.actor == "alice" && r.action == "exec.run"),
        "成功执行必须落审计"
    );
    assert!(
        rows.iter().any(|r| r.actor == "mallory" && r.action == "exec.denied"),
        "被拒的尝试也必须落审计(否则看不见有人在敲门)"
    );
    std::env::remove_var("POLARIS_COLLAB_DB");
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

#[test]
fn clip_truncates_over_limit() {
    let (s, t) = clip(vec![b'x'; MAX_OUTPUT + 10]);
    assert!(t);
    assert_eq!(s.len(), MAX_OUTPUT);
    let (s2, t2) = clip(vec![b'y'; 10]);
    assert!(!t2);
    assert_eq!(s2.len(), 10);
}
