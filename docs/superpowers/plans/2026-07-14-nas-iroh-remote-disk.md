# NAS 经 iroh P2P 接入资源管理系统(隧道 + 远程盘浏览)实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: 用 superpowers:executing-plans 逐任务执行。步骤用 `- [ ]` 复选框跟踪。

**Goal:** 让群晖 NAS(100.78.103.101)作为 iroh 主机接入,桌面经 iroh P2P 直连它,并在「文件中心」里像浏览本机一样浏览/预览/下载 NAS 上的文件。

**Architecture:** NAS 侧改跑带 `collab-host + collab-net` 的 Polaris server,启 iroh `host_listen` 暴露 NodeId,并新增一个**路径关押(path-jailed)的文件浏览 HTTP API(fsface)**;桌面侧复用现成的 `collab_tunnel_connect(nodeId, port)` 在 `127.0.0.1:port` 开透明代理,前端「文件中心」新增一个「远程源」面板,经该代理端口调 fsface API 列目录/预览/下载。传输隐形:局域网→P2P 打洞→中继由 iroh 自动选档。

**Tech Stack:** Rust(axum / iroh / tokio)· Vue 3 + Pinia · Tauri invoke · Docker(WSL 构建 + R2/GHCR 分发)· 现有 crates:`polaris-collab`(tunnel/http)、`src-tauri/src/hosting.rs`、前端 `FileCenter.vue`。

---

## 关键前置事实(已诊断)

- NAS `100.78.103.101:8080` 可达,`/api/health`=`ok`,但**所有 `/api/collab/*` 落 SPA 兜底** → 现镜像**未编 `collab-host`/`collab-net`**,无 iroh 端点、无 NodeId。→ 必须重建+重部署 NAS 镜像。
- 桌面端已有原语:`collab_tunnel_connect(hostNodeId, listenPort)`(`polaris-collab/src/collab/commands.rs:126`)、`collab_tunnel_status`、`collab_device_node_id`。**桌面侧连接底座是现成的**。
- 服务端**无任何文件浏览 HTTP API**(`read_dir` 全在本地盘点)→ fsface 为净新增。
- Server flavor features:`server = ["collab-host", "polaris-fable/server"]`;iroh 需额外 `collab-net`;云机构建口径 `--features server,collab-net`(`src-tauri/Cargo.toml:163-186`)。
- 隧道模型:主机 `host_listen` 把每条 iroh 流转发到本机 `upstream_addr()`(默认容器内 server 8080/8484);客户端 `client_proxy` 在 `127.0.0.1:port` 起代理(`tunnel.rs:209/317`)。**故 fsface 挂在同一个 8080 server 上即可随隧道透传,无需额外端口**(隧道只放行一个上游端口,契合 http.rs:2411 注释)。

---

## 文件结构地图(创建 / 修改)

| 文件 | 职责 | 动作 |
|---|---|---|
| `src-tauri/crates/polaris-collab/src/collab/fsface.rs` | **新**:路径关押的文件浏览 —— `list/stat/read/download`,根白名单 + 防穿越 fail-closed | 创建 |
| `src-tauri/crates/polaris-collab/src/collab/http.rs` | 挂 fsface 路由(`/api/fs/*`,owner 鉴权);`collab-host` 下编译 | 修改 |
| `src-tauri/crates/polaris-collab/src/collab/mod.rs` | `pub mod fsface;`(collab-host 门控) | 修改 |
| `src-tauri/src/hosting.rs` | 桌面主机也挂 fsface(自测用);读取 `POLARIS_FS_ROOTS` | 修改(小) |
| `src/features/collab/fsapi.ts` | **新**:前端 fsface 客户端(list/read/download,带 base+token) | 创建 |
| `src/features/interconnect/InterconnectView.vue` | 「设备与授权」页新增「接入远程主机 / NAS」入口:填连接码/NodeId → `collab_tunnel_connect` → 持久化远程源 | 修改 |
| `src/features/interconnect/remoteSources.ts` | **新**:远程源持久化(localStorage:name/nodeId/port/base),跨页共享 | 创建 |
| `src/components/FileCenter.vue` | 顶部源切换加「远程源(NAS)」,渲染 fsface 目录树 + 预览 + 下载 | 修改 |
| `docker/`(独立 docker 仓)/ `image.yml` | 新增 `server,collab-net` 构建目标 → GHCR/R2 | 修改(基础设施) |
| NAS `/volume1/tx/群晖/docker-compose.yml` | 换镜像 + 挂载存储卷 + 环境变量(FS_ROOTS/端口) | 修改(基础设施) |

---

## 阶段总览与门禁

- **Phase A(桌面侧,零风险,先做)**:远程源持久化 + 互联页「接入远程主机」入口 + 连接/状态 UI。不碰 NAS,可对**本机第二个 collab-net server** 自测。
- **Phase B(服务端,可先做)**:fsface 文件浏览 API(路径关押);桌面主机也挂上,本机自测浏览。
- **Phase C(前端,依赖 A+B)**:FileCenter 远程源面板(目录树/预览/下载)。
- **Phase D(基础设施,需你明确放行)**:重建带 `collab-net` 的 server 镜像 → 部署到 NAS → bootstrap owner → 启 iroh host → 拿 NodeId。**动生产 NAS,单独确认后执行**。
- **Phase E(可选,后续)**:WebDAV 让 NAS 挂成系统盘 Z:(OS 级挂载)。

> 建议执行序:A → B →(本机自测端到端)→ C → D(真机接 NAS)。A/B/C 全程不动 NAS,先在**本机跑两个 server 实例**验证隧道+fsface+远程源全链路,再上 NAS。

---

## Phase A — 桌面侧远程源与接入入口

### Task A1:远程源持久化模块

**Files:**
- Create: `src/features/interconnect/remoteSources.ts`

- [ ] **Step 1:写模块(含类型 + CRUD + localStorage)**

```ts
// src/features/interconnect/remoteSources.ts
// 远程源:一台经 iroh 隧道接入的主机(NAS/另一台电脑)。桌面侧持久化其连接参数,
// 供「互联」页展示与「文件中心」远程浏览共用。传输隐形,用户只认名字。
export interface RemoteSource {
  id: string;          // 稳定 id(nanoid/时间戳)
  name: string;        // 展示名,如「群晖 NAS」
  nodeId: string;      // iroh 主机 NodeId(z-base32)
  port: number;        // 本地代理端口(127.0.0.1:port)
  token: string;       // 连上该主机的 owner 令牌(fsface 鉴权)
  createdAt: number;
}

const KEY = "polaris.interconnect.remoteSources.v1";

export function loadRemoteSources(): RemoteSource[] {
  try { return JSON.parse(localStorage.getItem(KEY) || "[]"); } catch { return []; }
}
export function saveRemoteSources(list: RemoteSource[]): void {
  try { localStorage.setItem(KEY, JSON.stringify(list)); } catch { /* storage 不可用 */ }
}
export function upsertRemoteSource(s: RemoteSource): RemoteSource[] {
  const list = loadRemoteSources().filter((x) => x.id !== s.id);
  list.push(s);
  saveRemoteSources(list);
  return list;
}
export function removeRemoteSource(id: string): RemoteSource[] {
  const list = loadRemoteSources().filter((x) => x.id !== id);
  saveRemoteSources(list);
  return list;
}
/** 本地代理基址;fsapi/collab api 用它当 base。 */
export function remoteBase(s: RemoteSource): string {
  return `http://127.0.0.1:${s.port}`;
}
```

- [ ] **Step 2:类型检查**

Run: `npx vue-tsc --noEmit`
Expected: EXIT 0

- [ ] **Step 3:提交**

```bash
git add src/features/interconnect/remoteSources.ts
git commit -m "feat(interconnect): 远程源持久化模块(RemoteSource CRUD)"
```

### Task A2:互联页「接入远程主机 / NAS」入口

**Files:**
- Modify: `src/features/interconnect/InterconnectView.vue`(「设备与授权」tab 内加一段)

- [ ] **Step 1:script 增连接逻辑**

在 `<script setup>` 内加(复用现有 `invoke`、`toast`、`remoteSources`):

```ts
import { invoke } from "../../tauri";
import { loadRemoteSources, upsertRemoteSource, removeRemoteSource, type RemoteSource } from "./remoteSources";

const remotes = ref<RemoteSource[]>(loadRemoteSources());
const addForm = reactive({ name: "群晖 NAS", nodeId: "", token: "", open: false });
let portSeq = 18620; // 本地代理端口起点,逐个 +1 避冲突

async function connectRemote() {
  const nodeId = addForm.nodeId.trim();
  if (!nodeId) { toast.error("请填 NAS 的 iroh NodeId(或连接码)"); return; }
  const port = portSeq++;
  try {
    await invoke("collab_tunnel_connect", { hostNodeId: nodeId, listenPort: port });
    const src: RemoteSource = {
      id: `rs-${Date.now()}`, name: addForm.name.trim() || "远程主机",
      nodeId, port, token: addForm.token.trim(), createdAt: Date.now(),
    };
    remotes.value = upsertRemoteSource(src);
    addForm.open = false; addForm.nodeId = ""; addForm.token = "";
    toast.info(`已发起 iroh P2P 连接 ${src.name} —— 到「文件中心 · 远程源」浏览它的盘`);
  } catch (e) { toast.error(`连接失败:${(e as Error).message}`); }
}
function forgetRemote(s: RemoteSource) {
  if (!confirm(`断开并移除「${s.name}」?`)) return;
  remotes.value = removeRemoteSource(s.id);
}
```

- [ ] **Step 2:template 在「设备与授权」tab 末尾加卡片**

```html
<section class="glass grant-note">
  <div class="gn-head"><Network :size="15" :stroke-width="1.9" /> 接入远程主机 / NAS(iroh P2P)</div>
  <p class="foot-note" style="margin-top:0">粘 NAS 的连接码或 NodeId,系统经 iroh 打洞直连(打不通走中继),连上后到「文件中心 · 远程源」浏览它的盘。</p>
  <div v-for="s in remotes" :key="s.id" class="dev-line" style="justify-content:space-between">
    <span class="conn t-p2p"><Zap :size="12" :stroke-width="2" /> {{ s.name }}</span>
    <span class="dev-node">127.0.0.1:{{ s.port }}</span>
    <button class="b danger" @click="forgetRemote(s)"><ShieldOff :size="13" /></button>
  </div>
  <div v-if="addForm.open" class="auth-form" style="margin-top:10px">
    <input v-model="addForm.name" class="af-inp" placeholder="名称(群晖 NAS)" />
    <input v-model="addForm.nodeId" class="af-inp" placeholder="NAS iroh NodeId / 连接码" />
    <input v-model="addForm.token" class="af-inp" placeholder="owner 令牌(浏览鉴权)" />
    <button class="cta" @click="connectRemote"><Zap :size="15" /> 发起 iroh 连接</button>
  </div>
  <button v-else class="pill ghost" style="margin-top:10px" @click="addForm.open = true">+ 接入一台 NAS/主机</button>
</section>
```

- [ ] **Step 3:类型检查 + 提交**

Run: `npx vue-tsc --noEmit` → EXIT 0

```bash
git add src/features/interconnect/InterconnectView.vue
git commit -m "feat(interconnect): 接入远程主机/NAS 入口(iroh 连接 + 远程源登记)"
```

---

## Phase B — 服务端 fsface 文件浏览 API(路径关押)

### Task B1:fsface 核心(纯函数 + 单测先行)

**Files:**
- Create: `src-tauri/crates/polaris-collab/src/collab/fsface.rs`
- Test: 同文件 `#[cfg(test)] mod tests`

- [ ] **Step 1:写失败测试(路径关押:防穿越 + 根外拒绝)**

```rust
// fsface.rs 末尾
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn jail_rejects_traversal_and_outside() {
        let root = PathBuf::from("/data/share");
        // 正常子路径通过
        assert!(resolve_jailed(&[root.clone()], "sub/a.txt").is_ok());
        // .. 穿越被拒
        assert!(resolve_jailed(&[root.clone()], "../etc/passwd").is_err());
        assert!(resolve_jailed(&[root.clone()], "sub/../../x").is_err());
        // 绝对路径逃逸被拒
        assert!(resolve_jailed(&[root.clone()], "/etc/passwd").is_err());
    }
}
```

- [ ] **Step 2:跑测试确认失败**

Run: `cargo test -p polaris-collab --no-default-features --features collab-host fsface`
Expected: FAIL(`resolve_jailed` 未定义)

- [ ] **Step 3:实现 fsface 核心**

```rust
//! collab/fsface.rs —— 路径关押的远程文件浏览(隧道另一端的「盘」)。
//! 只在 collab-host 下编译。根白名单来自 POLARIS_FS_ROOTS(冒号/分号分隔);
//! 所有路径先规范化再校验落在某个根内,fail-closed:穿越/根外/符号链接逃逸一律 403。
#![cfg(feature = "collab-host")]

use std::path::{Component, Path, PathBuf};
use serde::Serialize;

#[derive(Serialize)]
pub struct FsEntry { pub name: String, pub is_dir: bool, pub size: u64, pub mtime: u64 }

/// 配置的浏览根(POLARIS_FS_ROOTS)。空 = 关闭远程浏览(返回空,fail-closed)。
pub fn roots() -> Vec<PathBuf> {
    std::env::var("POLARIS_FS_ROOTS").ok().into_iter()
        .flat_map(|s| s.split(|c| c == ':' || c == ';').map(|p| PathBuf::from(p.trim())).collect::<Vec<_>>())
        .filter(|p| !p.as_os_str().is_empty())
        .collect()
}

/// 把相对请求路径关押进某个根:规范化(去 . / 拒 ..)后拼到根上,再确认仍在根内。
pub fn resolve_jailed(roots: &[PathBuf], rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim_start_matches(['/', '\\']);
    let mut safe = PathBuf::new();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(c) => safe.push(c),
            Component::CurDir => {}
            _ => return Err("非法路径(禁止 .. / 绝对路径)".into()),
        }
    }
    for root in roots {
        let full = root.join(&safe);
        // 规范化后仍须以 root 为前缀(挡符号链接逃逸尽力而为;真实 canonicalize 见下)
        if let Ok(canon) = full.canonicalize() {
            if let Ok(rc) = root.canonicalize() {
                if canon.starts_with(&rc) { return Ok(canon); }
            }
        } else if roots.len() == 1 {
            // 目标还不存在(如即将读的文件)时退回前缀判断
            let joined = root.join(&safe);
            if joined.starts_with(root) { return Ok(joined); }
        }
    }
    Err("路径不在允许的浏览根内".into())
}

/// 列目录(相对根)。root_index=第几个根(前端在多根间切换)。
pub fn list(rel: &str) -> Result<Vec<FsEntry>, String> {
    let rs = roots();
    if rs.is_empty() { return Err("本机未开放远程浏览(POLARIS_FS_ROOTS 未设)".into()); }
    let dir = resolve_jailed(&rs, rel)?;
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).map_err(|e| format!("读目录失败: {e}"))? {
        let e = match e { Ok(e) => e, Err(_) => continue };
        let md = match e.metadata() { Ok(m) => m, Err(_) => continue };
        let mtime = md.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs()).unwrap_or(0);
        out.push(FsEntry {
            name: e.file_name().to_string_lossy().to_string(),
            is_dir: md.is_dir(), size: md.len(), mtime,
        });
    }
    out.sort_by(|a, b| (b.is_dir, a.name.to_lowercase()).cmp(&(a.is_dir, b.name.to_lowercase())));
    Ok(out)
}

/// 读文件字节(供预览/下载)。上限 512MB 防误读巨物 OOM。
pub fn read_bytes(rel: &str) -> Result<Vec<u8>, String> {
    let rs = roots();
    let f = resolve_jailed(&rs, rel)?;
    let md = std::fs::metadata(&f).map_err(|e| format!("stat 失败: {e}"))?;
    if md.len() > 512 * 1024 * 1024 { return Err("文件过大(>512MB),请用下载".into()); }
    std::fs::read(&f).map_err(|e| format!("读文件失败: {e}"))
}
```

- [ ] **Step 4:跑测试确认通过**

Run: `cargo test -p polaris-collab --no-default-features --features collab-host fsface`
Expected: PASS(`jail_rejects_traversal_and_outside`)

- [ ] **Step 5:提交**

```bash
git add src-tauri/crates/polaris-collab/src/collab/fsface.rs
git commit -m "feat(fsface): 路径关押的远程文件浏览核心(list/read + 防穿越单测)"
```

### Task B2:挂 HTTP 路由(owner 鉴权)

**Files:**
- Modify: `src-tauri/crates/polaris-collab/src/collab/mod.rs`(加 `pub mod fsface;`,collab-host 门控)
- Modify: `src-tauri/crates/polaris-collab/src/collab/http.rs`(加 3 条路由 + handler)

- [ ] **Step 1:mod.rs 导出**

```rust
// mod.rs,与 http 同一 cfg 段
#[cfg(feature = "collab-host")]
pub mod fsface;
```

- [ ] **Step 2:http.rs 加 handler(参照现有 owner 鉴权中间件写法)**

在 http.rs 找一个已有的 owner-gated GET handler 作模板(如 `admin_devices`),照抄鉴权提取,新增:

```rust
// GET /api/fs/list?path=<rel>
async fn fs_list_api(headers: HeaderMap, State(st): State<AppState>, Query(q): Query<std::collections::HashMap<String,String>>) -> Response {
    if let Err(r) = require_owner(&headers, &st).await { return r; }   // 复用现有 owner 闸
    let rel = q.get("path").map(|s| s.as_str()).unwrap_or("");
    match crate::collab::fsface::list(rel) {
        Ok(v) => Json(json!({ "entries": v })).into_response(),
        Err(e) => (StatusCode::FORBIDDEN, Json(json!({ "error": e }))).into_response(),
    }
}
// GET /api/fs/read?path=<rel>  → 原始字节(前端按 mime 预览/下载)
async fn fs_read_api(headers: HeaderMap, State(st): State<AppState>, Query(q): Query<std::collections::HashMap<String,String>>) -> Response {
    if let Err(r) = require_owner(&headers, &st).await { return r; }
    let rel = q.get("path").cloned().unwrap_or_default();
    match crate::collab::fsface::read_bytes(&rel) {
        Ok(bytes) => ([(axum::http::header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response(),
        Err(e) => (StatusCode::FORBIDDEN, Json(json!({ "error": e }))).into_response(),
    }
}
```

> 注:`require_owner` 若不存在,照 admin handler 里现有的「取 Bearer → 查会话 → 校 role」几行内联即可。fsface 路由**不加 collab-net cfg**(纯文件访问,仅需 collab-host),这样桌面主机也能挂上自测。

- [ ] **Step 3:注册路由(接 http.rs:2423 那段 `.route(...)` 链后)**

```rust
        .route("/api/fs/list", get(fs_list_api))
        .route("/api/fs/read", get(fs_read_api));
```

- [ ] **Step 4:双 flavor 编译**

Run(桌面):`cd src-tauri && cargo build --no-default-features --features desktop`
Run(服务端):`cargo build --no-default-features --features server,collab-net`
Expected: 均 Finished 无 error

- [ ] **Step 5:提交**

```bash
git add src-tauri/crates/polaris-collab/src/collab/mod.rs src-tauri/crates/polaris-collab/src/collab/http.rs
git commit -m "feat(fsface): /api/fs/list + /api/fs/read HTTP 路由(owner 鉴权,collab-host)"
```

### Task B3:桌面主机也开放浏览(本机自测钩子)

**Files:**
- Modify: `src-tauri/src/hosting.rs`(启动时若设了 `POLARIS_FS_ROOTS` 无需改代码,路由已挂;此步仅加一行日志确认)

- [ ] **Step 1:hosting 启动日志**

在 hosting 起 axum 成功处加:
```rust
if !crate::collab::fsface::roots().is_empty() {
    eprintln!("[fsface] 远程浏览已开放,根: {:?}", crate::collab::fsface::roots());
}
```
Expected: 设 `POLARIS_FS_ROOTS=D:\shared` 起桌面主机时打印该行。

- [ ] **Step 2:本机自测(curl 经 owner 令牌)**

```powershell
# 桌面已是主机、已登录 owner,拿到 token 与端口后:
$env:POLARIS_FS_ROOTS="D:\shared"   # 起主机前设
curl "http://127.0.0.1:<port>/api/fs/list?path=" -H "Authorization: Bearer <token>"
```
Expected: 返回 `{"entries":[...]}` 列出 D:\shared 下的文件。

- [ ] **Step 3:提交**

```bash
git add src-tauri/src/hosting.rs
git commit -m "chore(fsface): 桌面主机启动打印远程浏览根(自测)"
```

---

## Phase C — 文件中心「远程源」浏览面板

### Task C1:前端 fsface 客户端

**Files:**
- Create: `src/features/collab/fsapi.ts`

- [ ] **Step 1:写客户端**

```ts
// src/features/collab/fsapi.ts —— 经隧道代理端口调 NAS 的 fsface。
import { type RemoteSource, remoteBase } from "../interconnect/remoteSources";

export interface FsEntry { name: string; is_dir: boolean; size: number; mtime: number }

async function req(s: RemoteSource, path: string, rel: string): Promise<Response> {
  const u = `${remoteBase(s)}${path}?path=${encodeURIComponent(rel)}`;
  return fetch(u, { headers: s.token ? { Authorization: `Bearer ${s.token}` } : {} });
}

export async function fsList(s: RemoteSource, rel: string): Promise<FsEntry[]> {
  const r = await req(s, "/api/fs/list", rel);
  if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error || `列目录失败(${r.status})`);
  return (await r.json()).entries as FsEntry[];
}
/** 下载 URL(直接给 <a download> 或 tauri 保存);带 token 走 fetch→blob。 */
export async function fsDownload(s: RemoteSource, rel: string): Promise<Blob> {
  const r = await req(s, "/api/fs/read", rel);
  if (!r.ok) throw new Error(`下载失败(${r.status})`);
  return await r.blob();
}
```

- [ ] **Step 2:类型检查 + 提交**

Run: `npx vue-tsc --noEmit` → EXIT 0
```bash
git add src/features/collab/fsapi.ts
git commit -m "feat(fsapi): 前端 fsface 客户端(列目录/下载,经隧道端口)"
```

### Task C2:FileCenter 远程源面板

**Files:**
- Modify: `src/components/FileCenter.vue`

- [ ] **Step 1:顶部源切换加「远程源」**

读 `loadRemoteSources()`,在现有本机/核心层 tab 旁加每个远程源一个 chip;选中后进入远程浏览态。

- [ ] **Step 2:远程浏览态渲染目录**

```ts
import { loadRemoteSources } from "../features/interconnect/remoteSources";
import { fsList, fsDownload, type FsEntry } from "../features/collab/fsapi";
const remoteSel = ref<RemoteSource | null>(null);
const rcwd = ref("");            // 当前相对路径
const rentries = ref<FsEntry[]>([]);
async function openRemote(s: RemoteSource, rel = "") {
  remoteSel.value = s; rcwd.value = rel;
  try { rentries.value = await fsList(s, rel); }
  catch (e) { toast.error((e as Error).message); rentries.value = []; }
}
function enterDir(e: FsEntry) { if (e.is_dir) openRemote(remoteSel.value!, rcwd.value ? `${rcwd.value}/${e.name}` : e.name); }
async function download(e: FsEntry) {
  const blob = await fsDownload(remoteSel.value!, rcwd.value ? `${rcwd.value}/${e.name}` : e.name);
  const url = URL.createObjectURL(blob); const a = document.createElement("a");
  a.href = url; a.download = e.name; a.click(); URL.revokeObjectURL(url);
}
```

- [ ] **Step 3:模板(面包屑 + 列表,复用文件中心现有行样式)**

```html
<div v-if="remoteSel" class="remote-browse">
  <div class="crumb">{{ remoteSel.name }} › {{ rcwd || "根" }}</div>
  <div class="fbrow" v-for="e in rentries" :key="e.name" @dblclick="enterDir(e)">
    <span class="fi">{{ e.is_dir ? '📁' : '📄' }}</span><span>{{ e.name }}</span>
    <span class="sz">{{ e.is_dir ? '' : (e.size/1048576).toFixed(1)+' MB' }}</span>
    <button v-if="!e.is_dir" class="pill ghost sm" @click.stop="download(e)">下载</button>
  </div>
</div>
```

- [ ] **Step 4:类型检查 + 提交**

Run: `npx vue-tsc --noEmit` → EXIT 0
```bash
git add src/components/FileCenter.vue
git commit -m "feat(filecenter): 远程源浏览面板(NAS 目录树 + 下载,经 iroh 隧道)"
```

### Task C3:本机端到端自测(不碰 NAS)

- [ ] **Step 1:本机起第二个 server 实例当「假 NAS」**

```powershell
$env:POLARIS_FS_ROOTS="D:\datasets"
# 用 server flavor 起在另一端口,并起 iroh host(collab_host_start),拿它的 node_id 与 owner token
```

- [ ] **Step 2:桌面互联页「接入远程主机」填该 node_id + token → 连**
Expected: 互联页出现远程源;隧道 status=connected。

- [ ] **Step 3:文件中心切到该远程源 → 列出 D:\datasets → 进目录 → 下载一个文件**
Expected: 目录正确、下载成功、内容一致。**至此隧道+fsface+远程源全链路在本机验证通过。**

---

## Phase D — 重建 + 部署 NAS 镜像(动生产,需明确放行)

> ⚠ 本阶段改动你的生产 NAS。执行前单独确认。所有命令在 WSL 里跑(出包铁律:WSL 构建)。

### Task D1:构建带 collab-net 的 server 镜像

- [ ] **Step 1:在 docker 独立仓/CI 加构建目标**
镜像构建口径:`cargo build --release --no-default-features --features server,collab-net`(对齐云机构建;memory: `--features server,collab-net`)。
- [ ] **Step 2:WSL 构建镜像**(参照 memory `cloud-relay-server-image-r2`:先缓存基础镜像绕 WSL2 拉取慢)
```bash
# WSL
docker build -t polaris-server:collabnet -f docker/server.Dockerfile .
```
- [ ] **Step 3:推送分发**(GHCR 或 R2 分片,参照 memory `docker-r2-self-update` / `nas-install-both-methods`)

### Task D2:NAS 部署 + 起 iroh host

- [ ] **Step 1:SSH 上 NAS**(memory:`ssh zz@100.78.103.101`,base64 注入绕 PS 引号)
- [ ] **Step 2:改 compose:换镜像 + 挂存储卷 + 环境变量**
```yaml
# /volume1/tx/群晖/docker-compose.yml(真 compose 见 memory nas-polaris-datasets-mount)
services:
  polaris:
    image: polaris-server:collabnet
    ports: ["8080:8080"]
    volumes:
      - /volume1/tx:/data/tx:ro          # 要浏览的 NAS 存储(只读更安全)
      - /volume1/polaris:/root/Polaris    # 数据/host.key 持久化(NodeId 稳定)
    environment:
      - POLARIS_FS_ROOTS=/data/tx         # 开放浏览根
      - POLARIS_TUNNEL_STRICT=0
```
- [ ] **Step 3:recreate 容器**(memory:recreate 会丢 `--cpuset`/`--memory` 热改,注意补回)
```bash
docker compose up -d --force-recreate
```
- [ ] **Step 4:NAS 上 bootstrap owner + 起 iroh host,取 NodeId**
```bash
curl -s http://127.0.0.1:8080/api/collab/bootstrap -d '{"username":"nas","password":"<pw>","displayName":"NAS","hostSelf":true}'
curl -s http://127.0.0.1:8080/api/collab/tunnel/start -X POST -H "Authorization: Bearer <token>"
curl -s http://127.0.0.1:8080/api/collab/tunnel/status -H "Authorization: Bearer <token>"   # → node_id
```
Expected: `tunnel/status` 返回 `running=true` + `node_id=<NAS NodeId>`。

### Task D3:桌面真机接 NAS

- [ ] **Step 1:互联页「接入远程主机」填 NAS 的 node_id + owner token → 连**
Expected: 隧道 connected(局域网/中继由 iroh 自动选)。
- [ ] **Step 2:文件中心远程源 → 浏览 `/data/tx`(即 NAS 的 /volume1/tx)→ 下载验证**
Expected: 看到 NAS 上的数据集/文件,能进目录、能下载,内容一致。**NAS 经 iroh P2P 接入资源管理系统达成。**

---

## Phase E(可选,后续)— WebDAV 让 NAS 挂成系统盘 Z:

- fsface 之上加 WebDAV(PROPFIND/GET)→ Windows `net use Z: \\...` 或 Tauri 内挂载点,资源管理器里当本机盘。PRD 原型③的「真·透明挂载」。工作量单列,先不做。

---

## Self-Review(对照 PRD/需求)

- ✅ iroh P2P 接入:Phase D(NAS host_listen)+ 桌面 `collab_tunnel_connect`(Task A2)。
- ✅ 传输隐形/自动选档:复用 tunnel.rs 现成三档降级,UI 只作徽标。
- ✅ 远程盘浏览:fsface(B)+ FileCenter 远程源(C)。
- ✅ 安全 fail-closed:路径关押 + owner 鉴权 + 根白名单 + 只读挂载 + 512MB 上限(B1/B2/D2)。
- ✅ 不擅动生产:Phase A/B/C 全本机可测,D 单独门禁。
- ⚠ 待确认项:NAS owner 口令、要开放的具体卷、docker 分发渠道(GHCR vs R2)——执行 Phase D 前定。
- 缺口:WebDAV OS 级挂载列为 Phase E 可选,本计划的「浏览/下载」已满足「接入资源管理系统」。
