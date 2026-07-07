//! collab/checks.rs —— 任务卡检查工作流(CI-lite,GitHub status checks 式)。
//!
//! 原则:脚本说了算(用项目自己的开源工具链),AI 永不进 pass/fail 判定路径;
//! 工具缺失/超时 = skipped 而非 fail(不误伤);creative 档跳过构建类检查,
//! 只留密钥扫描+大文件闸(视频/游戏素材仓不为难)。
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::db::{now, open_db};
use rusqlite::params;

/// 单项检查超时(秒)。cargo check 冷缓存也该够;超时=skipped(timeout)。
const STEP_TIMEOUT: u64 = 600;
/// 输出只留尾部字节数(错误都在最后)。
const OUTPUT_TAIL: usize = 16 * 1024;

#[derive(serde::Serialize, Clone)]
pub struct CheckRun {
    pub name: String,
    pub status: String, // pass|fail|skipped|running
    pub output: String,
    pub started_at: i64,
    pub ended_at: i64,
}

/// 项目检查档位。
pub fn project_profile(project_id: i64) -> String {
    open_db()
        .ok()
        .and_then(|c| {
            c.query_row("SELECT check_profile FROM projects WHERE id=?1", [project_id], |r| {
                r.get::<_, String>(0)
            })
            .ok()
        })
        .unwrap_or_else(|| "code".into())
}

pub fn set_project_profile(project_id: i64, profile: &str) -> Result<(), String> {
    if !matches!(profile, "code" | "creative" | "off") {
        return Err("档位只能是 code/creative/off".into());
    }
    let conn = open_db()?;
    conn.execute("UPDATE projects SET check_profile=?1 WHERE id=?2", params![profile, project_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 读某轮检查结果。
pub fn list(task_id: i64, round: i64) -> Result<Vec<CheckRun>, String> {
    let conn = open_db()?;
    let mut stmt = conn
        .prepare("SELECT name,status,output,started_at,ended_at FROM check_runs WHERE task_id=?1 AND round=?2 ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![task_id, round], |r| {
            Ok(CheckRun { name: r.get(0)?, status: r.get(1)?, output: r.get(2)?, started_at: r.get(3)?, ended_at: r.get(4)? })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// 该轮检查是否全绿(pass/skipped 都算过;fail/running 不算)。无记录=未跑,不算过。
pub fn all_green(task_id: i64, round: i64) -> Result<bool, String> {
    let runs = list(task_id, round)?;
    if runs.is_empty() {
        return Ok(false);
    }
    Ok(runs.iter().all(|r| r.status == "pass" || r.status == "skipped"))
}

fn record(task_id: i64, round: i64, r: &CheckRun) {
    if let Ok(conn) = open_db() {
        let _ = conn.execute(
            "INSERT INTO check_runs(task_id,round,name,status,output,started_at,ended_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![task_id, round, r.name, r.status, r.output, r.started_at, r.ended_at],
        );
    }
}

fn clear_round(task_id: i64, round: i64) {
    if let Ok(conn) = open_db() {
        let _ = conn.execute("DELETE FROM check_runs WHERE task_id=?1 AND round=?2", params![task_id, round]);
    }
}

/// 同步跑完一轮检查(调用方决定放哪个线程)。emit 回调用于 collab:check 事件。
pub fn run_for_task(
    repo: &Path,
    branch: &str,
    task_id: i64,
    round: i64,
    profile: &str,
    emit: &dyn Fn(),
) -> Result<(), String> {
    if profile == "off" {
        return Ok(());
    }
    clear_round(task_id, round);
    // 临时 worktree(检完即删;失败也尽力清)。
    let wt = std::env::temp_dir().join(format!("polaris-check-{task_id}-{round}-{}", std::process::id()));
    let wts = wt.to_string_lossy().to_string();
    let _ = run_cmd(repo, &["worktree", "prune"], STEP_TIMEOUT);
    let out = run_cmd(repo, &["worktree", "add", "--detach", &wts, branch], STEP_TIMEOUT)?;
    if !out.0 {
        record(task_id, round, &CheckRun {
            name: "checkout".into(), status: "fail".into(),
            output: tail(&out.1), started_at: now(), ended_at: now(),
        });
        emit();
        return Err("worktree 检出失败".into());
    }
    let result = run_steps(&wt, task_id, round, profile, emit);
    let _ = run_cmd(repo, &["worktree", "remove", "--force", &wts], STEP_TIMEOUT);
    let _ = std::fs::remove_dir_all(&wt);
    result
}

fn run_steps(wt: &Path, task_id: i64, round: i64, profile: &str, emit: &dyn Fn()) -> Result<(), String> {
    // ① 密钥扫描 + ② 大文件闸:所有档位都跑(creative 只是上限放宽)。
    step(task_id, round, "密钥扫描", emit, || secret_scan(wt));
    let max_mb: u64 = if profile == "creative" { 500 } else { 50 };
    step(task_id, round, "大文件闸", emit, || big_file_scan(wt, max_mb));
    if profile == "creative" {
        return Ok(()); // 视频/游戏素材仓:不跑构建/静态检查
    }
    // ③ 工具链检查(探测到什么跑什么;工具缺失=skipped)。
    if wt.join("Cargo.toml").exists() {
        step(task_id, round, "cargo check", emit, || shell_step(wt, "cargo", &["check", "--quiet"]));
    }
    if wt.join("package.json").exists() {
        for script in ["lint", "typecheck", "build"] {
            if npm_script_exists(wt, script) {
                let name = format!("npm run {script}");
                step(task_id, round, &name, emit, || shell_step(wt, "npm", &["run", script]));
            }
        }
    }
    if wt.join("pyproject.toml").exists() || wt.join("ruff.toml").exists() {
        step(task_id, round, "ruff check", emit, || shell_step(wt, "ruff", &["check", "."]));
    }
    Ok(())
}

/// 单步骨架:先落 running(前端能看到进度),跑完覆写终态。
fn step(task_id: i64, round: i64, name: &str, emit: &dyn Fn(), f: impl FnOnce() -> (String, String)) {
    let started = now();
    record(task_id, round, &CheckRun {
        name: name.into(), status: "running".into(), output: String::new(),
        started_at: started, ended_at: 0,
    });
    emit();
    let (status, output) = f();
    if let Ok(conn) = open_db() {
        let _ = conn.execute(
            "UPDATE check_runs SET status=?1, output=?2, ended_at=?3 WHERE task_id=?4 AND round=?5 AND name=?6",
            params![status, output, now(), task_id, round, name],
        );
    }
    emit();
}

/// (status, output)。工具不存在 → skipped。
fn shell_step(cwd: &Path, prog: &str, args: &[&str]) -> (String, String) {
    match run_prog(cwd, prog, args, STEP_TIMEOUT) {
        Ok((true, out)) => ("pass".into(), tail(&out)),
        Ok((false, out)) => ("fail".into(), tail(&out)),
        Err(e) if e.contains("not found") || e.contains("找不到") || e.contains("cannot find") => {
            ("skipped".into(), format!("工具缺失,跳过: {e}"))
        }
        Err(e) if e.contains("timeout") => ("skipped".into(), format!("超时({STEP_TIMEOUT}s),跳过判定: {e}")),
        Err(e) => ("skipped".into(), format!("无法执行,跳过: {e}")),
    }
}

fn npm_script_exists(wt: &Path, script: &str) -> bool {
    std::fs::read_to_string(wt.join("package.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("scripts")?.get(script).map(|_| true))
        .unwrap_or(false)
}

/// git 命令(在 repo 目录)。返回 (成功?, 合并输出)。
fn run_cmd(repo: &Path, args: &[&str], timeout: u64) -> Result<(bool, String), String> {
    run_prog(repo, "git", args, timeout)
}

/// 后台线程排空一根管道(防子进程输出撑满 pipe 缓冲区死锁);非 UTF-8 有损转换。
fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut r) = pipe {
            let _ = r.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// 跨平台起进程 + 超时 kill。Windows 上 npm/npx/ruff 多为 .cmd/.exe,统一走 cmd /C。
fn run_prog(cwd: &Path, prog: &str, args: &[&str], timeout: u64) -> Result<(bool, String), String> {
    let mut cmd = if cfg!(windows) && prog != "git" && prog != "cargo" {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(prog).args(args);
        c
    } else {
        let mut c = Command::new(prog);
        c.args(args);
        c
    };
    let mut child = cmd
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{prog} not found / 启动失败: {e}"))?;
    // 边跑边排空 stdout/stderr:不排空的话输出超过 pipe 缓冲区子进程会写阻塞,永远等不到退出。
    let out_h = drain(child.stdout.take());
    let err_h = drain(child.stderr.take());
    let deadline = Instant::now() + Duration::from_secs(timeout);
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                let mut out = out_h.join().unwrap_or_default();
                out.push_str(&err_h.join().unwrap_or_default());
                return Ok((status.success(), out));
            }
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("timeout after {timeout}s"));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

fn tail(s: &str) -> String {
    if s.len() <= OUTPUT_TAIL {
        return s.to_string();
    }
    // 对齐到字符边界再切(工具输出常含中文,硬切字节会 panic)。
    let mut i = s.len() - OUTPUT_TAIL;
    while !s.is_char_boundary(i) {
        i += 1;
    }
    format!("…(截前略)…\n{}", &s[i..])
}

/// 密钥扫描:轻量正则内置(开源 gitleaks 的常见模式子集,不引外部依赖)。
/// 只扫文本文件、单文件≤2MB;命中即 fail 并指出文件。
fn secret_scan(wt: &Path) -> (String, String) {
    let pats: &[(&str, &str)] = &[
        ("AWS AccessKey", r"AKIA[0-9A-Z]{16}"),
        ("私钥块", r"-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----"),
        ("GitHub Token", r"ghp_[A-Za-z0-9]{36}"),
        ("Slack Token", r"xox[baprs]-[A-Za-z0-9-]{10,}"),
        ("通用 api_key 赋值", r#"(?i)(api[_-]?key|secret[_-]?key)\s*[:=]\s*['"][A-Za-z0-9_\-]{20,}['"]"#),
    ];
    let res: Vec<regex::Regex> = pats.iter().filter_map(|(_, p)| regex::Regex::new(p).ok()).collect();
    let mut hits = Vec::new();
    for entry in walkdir::WalkDir::new(wt)
        .into_iter()
        .filter_entry(|e| e.file_name().to_string_lossy() != ".git" && e.file_name().to_string_lossy() != "node_modules" && e.file_name().to_string_lossy() != "target")
        .flatten()
    {
        if !entry.file_type().is_file() { continue; }
        let Ok(md) = entry.metadata() else { continue };
        if md.len() > 2 * 1024 * 1024 { continue; }
        let Ok(text) = std::fs::read_to_string(entry.path()) else { continue }; // 非 UTF-8(二进制)自动跳过
        for (i, re) in res.iter().enumerate() {
            if re.is_match(&text) {
                hits.push(format!("{} → {}", pats[i].0, entry.path().strip_prefix(wt).unwrap_or(entry.path()).display()));
                if hits.len() >= 20 { break; }
            }
        }
        if hits.len() >= 20 { break; }
    }
    if hits.is_empty() { ("pass".into(), "未发现疑似密钥".into()) } else { ("fail".into(), hits.join("\n")) }
}

/// 大文件闸:超过上限的文件列出来。creative 档上限放宽(素材仓)。
fn big_file_scan(wt: &Path, max_mb: u64) -> (String, String) {
    let cap = max_mb * 1024 * 1024;
    let mut hits = Vec::new();
    for entry in walkdir::WalkDir::new(wt)
        .into_iter()
        .filter_entry(|e| e.file_name().to_string_lossy() != ".git")
        .flatten()
    {
        if !entry.file_type().is_file() { continue; }
        if let Ok(md) = entry.metadata() {
            if md.len() > cap {
                hits.push(format!("{}({} MB)", entry.path().strip_prefix(wt).unwrap_or(entry.path()).display(), md.len() / 1024 / 1024));
                if hits.len() >= 20 { break; }
            }
        }
    }
    if hits.is_empty() { ("pass".into(), format!("无 >{max_mb}MB 文件")) } else {
        ("fail".into(), format!("超过 {max_mb}MB 上限:\n{}", hits.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collab::db::TEST_LOCK;

    fn git(repo: &Path, args: &[&str]) {
        let out = Command::new("git").args(args).current_dir(repo).output().expect("git 启动失败");
        assert!(out.status.success(), "git {args:?} 失败: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// code 档:密钥扫描抓到假 AWS key=fail,大文件闸 pass,all_green=false;
    /// creative 档:只剩密钥扫描+大文件闸两项。
    #[test]
    fn checks_run_for_task_profiles() {
        let _g = TEST_LOCK.lock().unwrap();
        let tmpdb = std::env::temp_dir().join(format!("collab-checks-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&tmpdb);
        std::env::set_var("POLARIS_COLLAB_DB", &tmpdb);
        // check_runs 有 tasks 外键(foreign_keys=ON),先种上项目+卡。
        {
            let conn = open_db().unwrap();
            conn.execute("INSERT INTO projects(id,name,repo,created_at) VALUES(1,'t','',0)", []).unwrap();
            conn.execute("INSERT INTO tasks(id,project_id,title,created_at,updated_at) VALUES(1,1,'t',0,0)", []).unwrap();
        }
        // 临时 git 仓:main 干净,feat/t1 上有个带假 AWS key 的文件。
        let repo = std::env::temp_dir().join(format!("collab-checks-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init"]);
        git(&repo, &["config", "user.email", "t@test.local"]);
        git(&repo, &["config", "user.name", "tester"]);
        std::fs::write(repo.join("README.md"), "hello").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "init"]);
        git(&repo, &["checkout", "-b", "feat/t1"]);
        // 假 key 拼接构造,避免本仓库自己被密钥扫描类工具误报。
        let key = format!("{}{}", "AKIA", "ABCDEFGHIJKLMNOP");
        std::fs::write(repo.join("leak.txt"), format!("aws_id = {key}\n")).unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "leak"]);

        // code 档(仓里没有 Cargo.toml/package.json/pyproject → 只有两项内置检查)。
        run_for_task(&repo, "feat/t1", 1, 0, "code", &|| {}).unwrap();
        let runs = list(1, 0).unwrap();
        let sec = runs.iter().find(|r| r.name == "密钥扫描").expect("缺密钥扫描项");
        assert_eq!(sec.status, "fail", "假 AWS key 应被抓到: {}", sec.output);
        assert!(sec.output.contains("leak.txt"), "输出应指出文件: {}", sec.output);
        let big = runs.iter().find(|r| r.name == "大文件闸").expect("缺大文件闸项");
        assert_eq!(big.status, "pass");
        assert!(!all_green(1, 0).unwrap(), "有 fail 不该全绿");

        // creative 档:clear_round 后重跑,只剩两项(不跑构建/静态检查)。
        run_for_task(&repo, "feat/t1", 1, 0, "creative", &|| {}).unwrap();
        let runs = list(1, 0).unwrap();
        assert_eq!(runs.len(), 2, "creative 档只留密钥扫描+大文件闸");
        assert!(runs.iter().all(|r| r.name == "密钥扫描" || r.name == "大文件闸"));

        // off 档:直接返回,不清也不写。
        run_for_task(&repo, "feat/t1", 1, 0, "off", &|| {}).unwrap();
        assert_eq!(list(1, 0).unwrap().len(), 2);

        // 档位读写 + 非法值拒绝。
        assert_eq!(project_profile(1), "code");
        set_project_profile(1, "creative").unwrap();
        assert_eq!(project_profile(1), "creative");
        assert!(set_project_profile(1, "yolo").is_err());

        std::env::remove_var("POLARIS_COLLAB_DB");
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_file(&tmpdb);
    }
}
