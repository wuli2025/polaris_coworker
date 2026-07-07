# 任务卡检查工作流(GitHub status checks 式 CI-lite)实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 任务卡「提交送验」时主机自动在临时 worktree 跑脚本检查(构建/静态检查/密钥扫描/大文件闸,全开源零新依赖),结果像 PR status checks 一样亮在卡上并成为合并第四道闸;项目分 code/creative/off 三档 profile,视频/游戏类放宽。

**Architecture:** 新模块 `collab/checks.rs`:`git worktree add --detach` 检出分支 → 按项目文件探测工具链(Cargo/package.json scripts/ruff)→ 逐项跑(单项超时 kill,工具缺失=skipped 不算失败)→ 结果落新表 `check_runs` → emit `collab:check`。触发点在 task_submit handler(spawn 后台线程,不阻塞响应)。`merge_squash_api` 加检查闸(owner 可 `force:true` 强推留痕)。projects 表加 `check_profile` 列(code/creative/off)。AI 不进判定路径(已有 lead_ai_review 仅供参考)。

**关键事实:**
- `project_repo_path(project_id)`(http.rs)返回主机侧权威仓库路径;卡上有 `branch`。
- db.rs migrate 用 `CREATE TABLE IF NOT EXISTS` + PRAGMA 探测 ALTER 补列模式(team_id 先例);测试用 `TEST_LOCK`+`POLARIS_COLLAB_DB`。
- task_submit 由 http.rs `task_op!(task_submit, ...)` 宏生成;宏体内可拿 `st: &CollabState`(emit 用)。
- 六态卡:submit 后 state=review;检查结果按 (task_id, round) 记,打回重提自动开新一轮检查。
- Windows 上 npm/npx 是 .cmd,`std::process::Command` 直调会找不到 → 用 `cmd /C`;Unix 直接调。
- **编译前杀 polaris 进程、确认无并行 cargo;commit 只 add 点名文件;测 server 壳用 POLARIS_PORT=18080(8080 被占)。**

---

### Task 1: checks.rs 核心(探测+运行+落库)+ 表迁移

**Files:**
- Create: `src-tauri/src/collab/checks.rs`
- Modify: `src-tauri/src/collab/db.rs`(check_runs 表 + projects.check_profile 列)
- Modify: `src-tauri/src/collab/mod.rs`(挂模块)

- [ ] **Step 1: db.rs 迁移**

migrate() 建表批里加:
```sql
-- 任务卡检查工作流(GitHub status checks 式):每轮提交跑一组检查。
-- status: pass|fail|skipped|running。output 只留尾部(防爆库)。
CREATE TABLE IF NOT EXISTS check_runs(
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    round      INTEGER NOT NULL,
    name       TEXT NOT NULL,
    status     TEXT NOT NULL,
    output     TEXT NOT NULL DEFAULT '',
    started_at INTEGER NOT NULL,
    ended_at   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_checks_task ON check_runs(task_id, round);
```
仿 team_id 先例,PRAGMA 探测后补列:
```rust
// 增量列:projects.check_profile —— 检查档位 code(全套)/creative(视频游戏放宽)/off。
let has_profile: bool = conn
    .prepare("PRAGMA table_info(projects)")
    .and_then(|mut s| {
        s.query_map([], |r| r.get::<_, String>(1))
            .map(|rows| rows.flatten().any(|c| c == "check_profile"))
    })
    .unwrap_or(false);
if !has_profile {
    conn.execute("ALTER TABLE projects ADD COLUMN check_profile TEXT NOT NULL DEFAULT 'code'", [])
        .map_err(|e| format!("补 check_profile 列失败: {e}"))?;
}
```

- [ ] **Step 2: checks.rs**

```rust
//! collab/checks.rs —— 任务卡检查工作流(CI-lite,GitHub status checks 式)。
//!
//! 原则:脚本说了算(用项目自己的开源工具链),AI 永不进 pass/fail 判定路径;
//! 工具缺失/超时 = skipped 而非 fail(不误伤);creative 档跳过构建类检查,
//! 只留密钥扫描+大文件闸(视频/游戏素材仓不为难)。
use std::io::Read;
use std::path::{Path, PathBuf};
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
    let _ = run_cmd(repo, &["worktree", "prune"], STEP_TIMEOUT);
    let out = run_cmd(repo, &["worktree", "add", "--detach", &wt.to_string_lossy(), branch], STEP_TIMEOUT)?;
    if !out.0 {
        record(task_id, round, &CheckRun {
            name: "checkout".into(), status: "fail".into(),
            output: tail(&out.1), started_at: now(), ended_at: now(),
        });
        emit();
        return Err("worktree 检出失败".into());
    }
    let result = run_steps(&wt, task_id, round, profile, emit);
    let _ = run_cmd(repo, &["worktree", "remove", "--force", &wt.to_string_lossy()], STEP_TIMEOUT);
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
    let deadline = Instant::now() + Duration::from_secs(timeout);
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                let mut out = String::new();
                if let Some(mut o) = child.stdout.take() { let _ = o.read_to_string(&mut out); }
                if let Some(mut e) = child.stderr.take() { let mut s = String::new(); let _ = e.read_to_string(&mut s); out.push_str(&s); }
                return Ok((status.success(), out));
            }
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err(format!("timeout after {timeout}s"));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

fn tail(s: &str) -> String {
    if s.len() <= OUTPUT_TAIL { s.to_string() } else {
        format!("…(截前略)…\n{}", &s[s.len() - OUTPUT_TAIL..])
    }
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
```
mod.rs 加 `pub mod checks;`(无条件,walkdir/regex 都是既有无条件依赖)。

- [ ] **Step 3: 单测**(checks.rs tests:临时目录 git init + 提交一个带假 AWS key 的文件到分支,run_for_task profile=code,断言密钥扫描 fail、大文件 pass、all_green=false;再跑 creative 断言只有两项)。用 `db::TEST_LOCK`+临时库。

- [ ] **Step 4: 验证 + Commit**
```powershell
cargo test --manifest-path src-tauri\Cargo.toml --no-default-features --features server --lib collab::checks
git add src-tauri/src/collab/checks.rs src-tauri/src/collab/db.rs src-tauri/src/collab/mod.rs
git commit -m "feat(collab): 检查工作流核心(worktree跑开源工具链+密钥扫描+大文件闸,三档profile)"
```

---

### Task 2: 接线(提交触发 + 端点 + 合并闸)

**Files:**
- Modify: `src-tauri/src/collab/http.rs`

- [ ] **Step 1: task_submit 触发**

`task_op!(task_submit, ...)` 的闭包里,submit 成功后追加(闭包能拿 st):
```rust
// 触发本轮检查(后台线程,不阻塞提交响应;结果经 collab:check 推送)
let card2 = card.clone();
let app2 = st.app.clone();
std::thread::spawn(move || {
    let profile = crate::collab::checks::project_profile(card2.project_id);
    let repo = match crate::collab::http::project_repo_path(card2.project_id) {
        Ok(r) => r,
        Err(_) => return, // 项目没配仓库 → 无从检查,静默跳过
    };
    let emit = || {
        let _ = app2.emit("collab:check", serde_json::json!({"taskId": card2.id, "round": card2.round}));
    };
    let _ = crate::collab::checks::run_for_task(&repo, &card2.branch, card2.id, card2.round, &profile, &emit);
});
```
(project_repo_path 需改 `pub fn`。)

- [ ] **Step 2: 端点**
```rust
// GET /api/collab/checks?taskId= → {profile, round, runs:[...]}
// POST /api/collab/checks/rerun {taskId}
// POST /api/collab/checks/profile {projectId, profile}(管理者)
```
三个 handler 全按现有骨架(auth_ctx→ensure_member/can_admin→spawn_blocking);checks GET 取卡拿 project_id/round;rerun 复用 Step 1 的 spawn 逻辑;profile 设置走 `can_admin` 闸。路由挂 tasks 段后。

- [ ] **Step 3: 合并第四道闸**

`merge_squash_api` 验收闸之后加:
```rust
// 检查闸(GitHub required checks 式):最新一轮检查须全绿。
// profile=off 或项目没配仓库(跑不了)不拦;owner 可 force 强推(留痕审计)。
let force = v.get("force").and_then(|x| x.as_bool()).unwrap_or(false);
let profile = crate::collab::checks::project_profile(card.project_id);
if profile != "off" && !force {
    match crate::collab::checks::all_green(tid, card.round) {
        Ok(true) => {}
        Ok(false) => return Err("检查闸未过:本轮检查未全绿(或未跑完)。owner 可带 force 强推".into()),
        Err(_) => {}
    }
}
if force {
    crate::collab::db::audit(&ctx.username, "merge.force", &tid.to_string(), "跳过检查闸强推");
}
```
⚠ force 只对 can_admin 分支有意义(as_lead 路径不许 force——主 Agent 无权跳检查)。

- [ ] **Step 4: 双壳编译 + 全量 collab 测试 + Commit**
```powershell
cargo test --manifest-path src-tauri\Cargo.toml --no-default-features --features server --lib collab
cargo check --manifest-path src-tauri\Cargo.toml
git add src-tauri/src/collab/http.rs
git commit -m "feat(collab): 提交触发检查 + checks 端点 + 合并第四道闸(owner可force留痕)"
```

---

### Task 3: 前端(卡片徽章 + 抽屉详情 + 档位设置)

**Files:**
- Modify: `src/features/collab/api.ts`(CheckRun 类型 + 三接口)
- Modify: `src/features/collab/stores/collab.ts`(checks state + collab:check 事件)
- Modify: `src/features/collab/TaskBoard.vue`(卡片徽章 + 抽屉检查段)
- Modify: `src/features/collab/ProjectHome.vue`(概览 tab 加档位选择,管理者可改)

- [ ] **Step 1: api.ts**
```ts
export interface CheckRun {
  name: string;
  status: "pass" | "fail" | "skipped" | "running";
  output: string;
  started_at: number;
  ended_at: number;
}
// collabApi 加:
checks(taskId: number): Promise<{ profile: string; round: number; runs: CheckRun[] }>
checksRerun(taskId: number): Promise<{ ok: boolean }>
checksSetProfile(projectId: number, profile: string): Promise<{ ok: boolean }>
```

- [ ] **Step 2: store**
`checksByTask = ref<Record<number, CheckRun[]>>({})` + `refreshChecks(taskId)`;WS/listen 增 `collab:check` → refreshChecks(payload.taskId)(direct WS 的 onmessage 分支同步加)。`checkProfile = ref<string>("code")` 随 checks() 响应更新;`setCheckProfile(profile)` 调 API 后回写。导出。

- [ ] **Step 3: TaskBoard**
- 卡片(review/in_progress 态)角落加检查徽章:全绿 ✓(绿)、有 fail ✗(红)、有 running ●(琥珀,呼吸动画)。数据:打开项目时对 review 态卡片逐个 `refreshChecks`(懒:详情抽屉打开时必刷)。
- 详情抽屉「验收记录」段之前插「检查」段:逐项 name+status 徽章,点击展开 `<pre>` output(等宽小字,max-height 滚动);「重跑检查」按钮(管理者)。
- 合并按钮失败提示已有 toast 通道,检查闸报错文案后端已带;owner 看到检查未过时按钮旁给「强推(跳过检查)」二次确认入口,调 squash 带 `force:true`(api.ts squash 接口加可选 force 参数)。

- [ ] **Step 4: ProjectHome 概览加档位卡**
成员条下方(管理者可见 select):
```
检查档位:[代码(全套) | 创作(视频/游戏,放宽) | 关闭]
说明一行:创作档只保留密钥扫描与 500MB 大文件闸。
```
调 `collab.setCheckProfile`。

- [ ] **Step 5: 验证 + Commit**
```powershell
npx vue-tsc --noEmit && npm run build
git add src/features/collab/api.ts src/features/collab/stores/collab.ts src/features/collab/TaskBoard.vue src/features/collab/ProjectHome.vue
git commit -m "feat(collab-fe): 检查徽章+抽屉详情+重跑+档位设置+owner强推"
```

---

### Task 4: 端到端

- [ ] server 壳(18080+临时库):建项目(repo 指向一个临时 git 仓,里面放 package.json 无 scripts + 一个假 AWS key 文件在分支上)→ 建卡 claim→submit → 轮询 `GET /checks?taskId=` 至非 running → 断言密钥扫描 fail → squash 应被检查闸拒 → squash force:true 应过(owner)→ audit 里有 merge.force。
- [ ] creative 档位:`POST /checks/profile creative` → rerun → 只剩两项(密钥+大文件)。
- [ ] 桌面真机:交用户点验(卡片徽章/抽屉/档位下拉)。
- [ ] 回归:`cargo test --features server --lib collab` + `npx vue-tsc` + `npm run build` 全绿。

## Self-Review
- 脚本为主 ✓(全部检查是进程/正则,AI 零介入判定);开源 ✓(项目自带工具链+内置轻量扫描,零新 crate——regex/walkdir/serde_json 都已有);视频/游戏放宽 ✓(creative 档);报错回填到卡 ✓(check_runs+徽章+output 尾部)。
- 一致性:profile 值 code/creative/off;status pass/fail/skipped/running;事件 collab:check;端点 /api/collab/checks(GET)/checks/rerun/checks/profile;merge force 仅 can_admin。
- 取舍:不跑 `npm ci`/`cargo build --release` 这类重活(check/lint 级别足够挡明显坏损);pytest 不默认跑(时长不可控);worktree 并发同卡重跑未加互斥(v1 靠 clear_round 覆盖,极端并发只是结果交错,不损坏)。
