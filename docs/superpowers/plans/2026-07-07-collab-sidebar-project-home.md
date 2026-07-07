# 侧栏团队项目分区 + 项目主页 + 对话绑定(GitHub 式协作联动)实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 侧栏区分「团队项目」与「我的项目」,团队项目可展开看挂在其下的对话、点进 GitHub 式项目主页(概览/任务/讨论/成员四 tab,含动态时间线),对话与协作项目通过本地绑定桥打通。

**Architecture:** 两套项目系统(conv 本地 string id / collab 远端 number id)之间架"绑定桥":conv `Project` 加 `collab_project_id`+`collab_host` 字段(serde default 零迁移),首次进团队项目自动建同名本地项目绑定(类比 git clone)。侧栏新增团队项目分区(徽章计数由 `project_list` 响应携带,免 N 次请求);新视图 `collab_project`=ProjectHome,任务 tab 直嵌无 props 的 TaskBoard;动态时间线数据由新端点 `/api/collab/activity` 从 review_rounds+tasks 合成(不依赖 audit 格式)。

**Tech Stack:** Rust(axum/rusqlite,双壳共用 http.rs 模式)、Vue3+Pinia、无新依赖。

**关键事实(已勘察,勿再验证):**
- conv `Project` 在 `src-tauri/src/conv.rs:21-34`,`Conversation` :37-47(`project_id: String` 指本地项目);存储 `~/Polaris/data/state.json` 全局 RwLock,`#[serde(default)]` 加字段向后兼容。
- collab `CollabProject{id:number,name,repo,team_id,lead_expert_id,charter_path,archived}`(`src/features/collab/api.ts:49-59`);`projects::list_for(user_id,is_owner)` 在 `src-tauri/src/collab/projects.rs:65`。
- 侧栏 `src/components/Sidebar.vue`:项目分组区 :341-538,`sortedProjects`(:376)/`sortedConvs`(:246 经 `app.conversationsByProject`);点对话 `app.selectConversation`;协作入口仅 primaryNav 一个键(:58)。
- 视图路由:`ViewKey` union 在 `src/stores/app.ts:15-35`;`App.vue:365-393` v-if 链渲染,`CollabView` 懒加载(:54)。
- `TaskBoard.vue` 无 props,纯读 `useCollabStore()`;`collab.selectProject(id)`(collab.ts:379)会拉 tasks+members。
- `TeamMembers.vue` 是团队级(需 currentTeamId),项目成员用 `collab.members` 直接渲染。
- conv 命令在 Docker 走 server.rs `dispatch_sync`——**新增 conv 命令必须同步接 dispatch,否则 Docker 版调不到(铁律)**。
- 前端 conv API 映射在 `src/tauri.ts:1188-1253`(snake↔camel)。
- **编译前先确认没有并行会话在 cargo 编译(别抢 target/ 锁),并杀 polaris-app/polaris-server 进程。commit 只 add 点名文件。**

---

### Task 1: 后端 — project_list 带任务计数 + activity 端点

**Files:**
- Modify: `src-tauri/src/collab/projects.rs`(list_for 附 open/review 计数)
- Modify: `src-tauri/src/collab/tasks.rs`(新增 activity 合成查询 + 单测)
- Modify: `src-tauri/src/collab/http.rs`(activity handler + 路由)

- [ ] **Step 1: projects.rs — 计数**

`list_for` 返回的每个项目 JSON 附两个字段(在现有 SELECT 结果映射处,对每个项目补查一条聚合;项目数量级小,N+1 可接受):
```rust
// 侧栏徽章:进行中(pending+in_progress)与待验收(review)计数,GitHub 式 repo 列表体验。
let (open_cnt, review_cnt): (i64, i64) = conn
    .query_row(
        "SELECT
           SUM(CASE WHEN state IN ('pending','in_progress') THEN 1 ELSE 0 END),
           SUM(CASE WHEN state='review' THEN 1 ELSE 0 END)
         FROM tasks WHERE project_id=?1",
        [p.id],
        |r| Ok((r.get::<_, Option<i64>>(0)?.unwrap_or(0), r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
    )
    .unwrap_or((0, 0));
```
落进返回结构(若 list_for 返回 struct,给 struct 加 `pub open_count: i64, pub review_count: i64`;若返回 Value 直接 insert)。

- [ ] **Step 2: tasks.rs — activity 合成**

```rust
/// 项目动态时间线(GitHub activity feed 式):由验收轮次+任务状态合成,
/// 不读 audit 表(其 target 格式不稳定)。kind: review|task。
#[derive(serde::Serialize)]
pub struct ActivityItem {
    pub kind: String,
    pub actor: String,
    pub task_id: i64,
    pub title: String,
    pub detail: String, // review: pass/reject+round;task: state
    pub at: i64,
}

pub fn activity(project_id: i64, limit: i64) -> Result<Vec<ActivityItem>, String> {
    let conn = crate::collab::db::open_db()?;
    let mut items: Vec<ActivityItem> = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT r.reviewer, r.task_id, t.title, r.verdict, r.round, r.created_at
         FROM review_rounds r JOIN tasks t ON r.task_id=t.id
         WHERE t.project_id=?1 ORDER BY r.created_at DESC LIMIT ?2",
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([project_id, limit], |r| {
        Ok(ActivityItem {
            kind: "review".into(),
            actor: r.get(0)?, task_id: r.get(1)?, title: r.get(2)?,
            detail: format!("{} · 第{}轮", r.get::<_, String>(3)?, r.get::<_, i64>(4)?),
            at: r.get(5)?,
        })
    }).map_err(|e| e.to_string())?;
    items.extend(rows.flatten());
    let mut stmt = conn.prepare(
        "SELECT COALESCE(u.display_name, u.username, ''), t.id, t.title, t.state, t.updated_at
         FROM tasks t LEFT JOIN users u ON t.assignee=u.id
         WHERE t.project_id=?1 ORDER BY t.updated_at DESC LIMIT ?2",
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([project_id, limit], |r| {
        Ok(ActivityItem {
            kind: "task".into(),
            actor: r.get(0)?, task_id: r.get(1)?, title: r.get(2)?,
            detail: r.get(3)?, at: r.get(4)?,
        })
    }).map_err(|e| e.to_string())?;
    items.extend(rows.flatten());
    items.sort_by(|a, b| b.at.cmp(&a.at));
    items.truncate(limit as usize);
    Ok(items)
}
```
⚠ `COALESCE(u.display_name,...)`:display_name 是 NOT NULL DEFAULT ''——空串时想退 username 需 `CASE WHEN u.display_name='' THEN u.username ELSE u.display_name END`,实现时按此写。
单测(tasks.rs tests 模块,拿 `db::TEST_LOCK`+临时库):建项目/卡→claim→review→activity 返回两类条目、时序倒排。

- [ ] **Step 3: http.rs — 端点**

```rust
async fn collab_activity(
    State(state): State<CollabState>, headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(ctx) = auth_ctx(&state, &headers) else { return forbid(); };
    let Some(pid) = q.get("projectId").and_then(|s| s.parse::<i64>().ok()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"缺 projectId"}))).into_response();
    };
    if let Err(r) = ensure_member(&ctx, pid) { return r; }
    let limit = q.get("limit").and_then(|s| s.parse::<i64>().ok()).unwrap_or(30).clamp(1, 100);
    let out = tokio::task::spawn_blocking(move || crate::collab::tasks::activity(pid, limit).and_then(ok)).await;
    unwrap_api(out)
}
```
路由表 tasks 段后加:`.route("/api/collab/activity", get(collab_activity))`。

- [ ] **Step 4: 验证 + Commit**
```powershell
cargo test --manifest-path src-tauri\Cargo.toml --no-default-features --features server --lib collab
cargo check --manifest-path src-tauri\Cargo.toml
git add src-tauri/src/collab/projects.rs src-tauri/src/collab/tasks.rs src-tauri/src/collab/http.rs
git commit -m "feat(collab): 项目列表带任务计数 + /api/collab/activity 动态时间线端点"
```

---

### Task 2: 后端 — conv 项目绑定桥

**Files:**
- Modify: `src-tauri/src/conv.rs`
- Modify: `src-tauri/src/lib.rs`(注册命令)
- Modify: `src-tauri/src/server.rs`(dispatch_sync 接线)

- [ ] **Step 1: Project 加字段**(conv.rs:21-34 struct 内):
```rust
/// 绑定的协作项目 id(团队项目↔本地对话工作区之桥;None=普通本地项目)
#[serde(default)]
pub collab_project_id: Option<i64>,
/// 绑定时的协作主机 base(空=同源/未绑;换主机时据此识别失配)
#[serde(default)]
pub collab_host: String,
```

- [ ] **Step 2: 绑定命令**(conv.rs,仿 conv_create_project 写法):
```rust
/// 把本地项目绑到协作项目(团队项目首次打开时前端自动建同名项目并调本命令)。
#[tauri::command]
pub fn conv_project_bind_collab(
    project_id: String,
    collabProjectId: i64,
    collabHost: String,
) -> Result<Project, String> {
    let mut st = state_mut()?; // 按 conv.rs 现有锁写法取写锁
    let p = st.projects.iter_mut().find(|p| p.id == project_id).ok_or("项目不存在")?;
    p.collab_project_id = Some(collabProjectId);
    p.collab_host = collabHost;
    let out = p.clone();
    save_state(&st)?; // 按现有原子写函数名
    Ok(out)
}
```
(state_mut/save_state 用 conv.rs 现有的锁与落盘函数,名字以现文件为准。)

- [ ] **Step 3: 接线** lib.rs invoke_handler conv 段加 `conv::conv_project_bind_collab,`;server.rs `dispatch_sync` 的 conv 命令段加:
```rust
"conv_project_bind_collab" => ok(conv::conv_project_bind_collab(
    req_str(&args, "projectId")?,
    args.get("collabProjectId").and_then(|v| v.as_i64()).ok_or("缺 collabProjectId")?,
    req_str(&args, "collabHost").unwrap_or_default(),
)?),
```
(req_str 等取参器按 dispatch_sync 现有风格。)

- [ ] **Step 4: 验证 + Commit**
```powershell
cargo check --manifest-path src-tauri\Cargo.toml
cargo check --manifest-path src-tauri\Cargo.toml --no-default-features --features server
git add src-tauri/src/conv.rs src-tauri/src/lib.rs src-tauri/src/server.rs
git commit -m "feat(conv): 项目加 collab 绑定字段 + conv_project_bind_collab 命令(双壳接线)"
```

---

### Task 3: 前端数据层

**Files:**
- Modify: `src/tauri.ts`(Project 类型+映射)
- Modify: `src/stores/app.ts`(bind action + ViewKey)
- Modify: `src/features/collab/api.ts`(CollabProject 计数 + activity)
- Modify: `src/features/collab/stores/collab.ts`(activity action)

- [ ] **Step 1: tauri.ts** `Project`(:1188-1197)加 `collabProjectId?: number | null; collabHost?: string;`;`convApi` 的 raw 映射(:1239-1253)加 `collab_project_id`↔`collabProjectId`、`collab_host`↔`collabHost`;新增:
```ts
bindProjectCollab(projectId: string, collabProjectId: number, collabHost: string): Promise<Project> {
  return invoke("conv_project_bind_collab", { projectId, collabProjectId, collabHost });
}
```
(返回 raw 需过同一映射函数。)

- [ ] **Step 2: app.ts**
- `ViewKey` union 加 `"collab_project"`。
- action:
```ts
async function bindProjectToCollab(projectId: string, collabProjectId: number, collabHost: string) {
  const p = await convApi.bindProjectCollab(projectId, collabProjectId, collabHost);
  const i = projects.value.findIndex((x) => x.id === p.id);
  if (i >= 0) projects.value[i] = p;
}
```
- getter:`projectByCollabId(collabId: number)` 在 projects 里 find `collabProjectId === collabId`。
- 导出两者。

- [ ] **Step 3: collab api.ts** `CollabProject` 加 `open_count?: number; review_count?: number;`;`ActivityItem` 类型 + `activity(projectId: number, limit = 30): Promise<ActivityItem[]>` → GET `/api/collab/activity?projectId=&limit=`。

- [ ] **Step 4: collab store** 加:
```ts
const activity = ref<ActivityItem[]>([]);
async function refreshActivity() {
  if (currentProjectId.value == null) return;
  try { activity.value = await collabApi.activity(currentProjectId.value); } catch { activity.value = []; }
}
```
`selectProject` 里并发追加 `refreshActivity()`;WS `collab:task` 事件处理处顺带 `void refreshActivity()`。导出。

- [ ] **Step 5: 验证 + Commit**
```powershell
npx vue-tsc --noEmit
git add src/tauri.ts src/stores/app.ts src/features/collab/api.ts src/features/collab/stores/collab.ts
git commit -m "feat(collab-fe): 绑定桥数据层 + 项目计数/动态类型 + activity action"
```

---

### Task 4: ProjectHome 视图(GitHub repo 主页式)

**Files:**
- Create: `src/features/collab/ProjectHome.vue`
- Modify: `src/App.vue`(懒加载+渲染分支)

- [ ] **Step 1: ProjectHome.vue**

结构(脚本要点):
```ts
const collab = useCollabStore();
const app = useAppStore();
type Tab = "overview" | "tasks" | "talks" | "members";
const tab = ref<Tab>("overview");
const proj = computed(() => collab.currentProject);
// 六态统计
const stat = computed(() => {
  const by: Record<string, number> = {};
  for (const t of collab.tasks) by[t.state] = (by[t.state] ?? 0) + 1;
  return by;
});
// 绑定的本地项目与其对话
const bound = computed(() =>
  proj.value ? app.projectByCollabId(proj.value.id) : undefined
);
const talks = computed(() =>
  bound.value ? (app.conversationsByProject[bound.value.id] ?? []) : []
);
/** 确保绑定的本地项目存在(首次自动建同名项目,git clone 式) */
async function ensureBound(): Promise<string | null> {
  if (!proj.value) return null;
  if (bound.value) return bound.value.id;
  const p = await app.createProject(proj.value.name);
  await app.bindProjectToCollab(p.id, proj.value.id, collab.base);
  return p.id;
}
async function newTalk() {
  const pid = await ensureBound();
  if (!pid) return;
  await app.createConversation(pid); // 内部会 setView("chat")
}
function openTalk(c: Conversation) { app.selectConversation(c); }
onMounted(() => { void collab.init(); void collab.refreshActivity(); });
```
模板:头部(项目名 + 团队名 + repo mono + 徽章 open/review)+ tab 条 + 四分支:
- overview:统计块(六态各一 tile)+ 动态时间线(`collab.activity`:kind==review 用 History 图标显示 actor/verdict/title,task 用 CircleDot 显示 state 变化;fmtTime)+ 成员头像条(collab.members 名字首字圆片)。
- tasks:`<TaskBoard />`(直接引,零 props)。
- talks:对话列表(title + 相对时间,点击 openTalk)+「开新讨论」按钮(newTalk);空态文案「还没有讨论,开一个吧」。
- members:collab.members 简表(display_name/@username/role),仿 CollabView 左栏成员段样式。
样式:贴 CollabAdmin 的 card/tbl 体系,tab 条贴 CollabView 的 .tabs。

- [ ] **Step 2: App.vue 接线**
- 懒加载(CollabView 旁):`const ProjectHome = defineAsyncComponent(() => import("./features/collab/ProjectHome.vue"));`
- 渲染链 `collab` 分支旁加:`<ProjectHome v-else-if="mountedView === 'collab_project'" />`。

- [ ] **Step 3: 验证 + Commit**
```powershell
npx vue-tsc --noEmit
git add src/features/collab/ProjectHome.vue src/App.vue
git commit -m "feat(collab-fe): ProjectHome 项目主页(概览/任务/讨论/成员 + 动态时间线)"
```

---

### Task 5: 侧栏团队项目分区

**Files:**
- Modify: `src/components/Sidebar.vue`

- [ ] **Step 1: 数据接入**
script 引 `useCollabStore`;`onMounted` 补 `void collab.init();`(幂等,validated 闸已有)。
```ts
const collab = useCollabStore();
const teamProjects = computed(() => (collab.authed ? collab.projects.filter((p) => !p.archived) : []));
const expandedTeam = ref<Record<number, boolean>>({});
function openTeamProject(p: CollabProject) {
  void collab.selectProject(p.id);
  app.setView("collab_project");
}
function boundConvsOf(p: CollabProject) {
  const b = app.projectByCollabId(p.id);
  return b ? (app.conversationsByProject[b.id] ?? []) : [];
}
```
`sortedProjects`(我的项目区)过滤掉已绑定的本地项目防重复显示:`.filter((p) => p.collabProjectId == null)`。

- [ ] **Step 2: 模板**
现有项目区(proj-section)**之前**插团队分区:
```html
<div v-if="collab.authed && teamProjects.length" class="proj-section team-section">
  <div class="sec-head">团队项目</div>
  <div v-for="tp in teamProjects" :key="tp.id" class="proj">
    <div class="proj-row" :class="{ active: app.view === 'collab_project' && collab.currentProjectId === tp.id }">
      <button class="proj-caret" @click.stop="expandedTeam[tp.id] = !expandedTeam[tp.id]">
        <ChevronRight :size="12" :class="{ open: expandedTeam[tp.id] }" />
      </button>
      <button class="proj-name-btn" @click="openTeamProject(tp)">
        <UsersRound :size="13" /> {{ tp.name }}
      </button>
      <span v-if="(tp.open_count ?? 0) + (tp.review_count ?? 0) > 0" class="tp-badge">
        {{ tp.open_count || 0 }}<template v-if="tp.review_count"> · 验{{ tp.review_count }}</template>
      </span>
    </div>
    <div v-if="expandedTeam[tp.id]" class="team-convs">
      <div v-for="c in boundConvsOf(tp)" :key="c.id" class="conv-row" @click="app.selectConversation(c)">
        {{ c.title || "未命名对话" }}
      </div>
      <div v-if="!boundConvsOf(tp).length" class="team-empty">还没有讨论 · 点项目名进主页开一个</div>
    </div>
  </div>
</div>
<div v-else-if="collab.authed === false && isTauriLike" ...>
```
(未登录不显示分区——协作入口 primaryNav 已有,不再加提示行,免噪音。conv-row/proj-row 样式尽量复用现有 class;补 `.tp-badge`(金色小胶囊,同 badge-host 色系)与 `.team-section .sec-head` 小节标题样式,贴现有侧栏字号体系。图标 `UsersRound`/`ChevronRight` 已在项目里用过,按需 import。)

- [ ] **Step 3: 验证 + Commit**
```powershell
npx vue-tsc --noEmit && npm run build
git add src/components/Sidebar.vue
git commit -m "feat(collab-fe): 侧栏团队项目分区(徽章计数+展开讨论+去重我的项目)"
```

---

### Task 6: 端到端验证

- [ ] **Step 1: 后端链路(server 壳,临时库)**
起 polaris-server(POLARIS_COLLAB_DB 临时)→ bootstrap → 建团队/项目 → 建卡 claim/review → `GET /api/collab/projects`(带 open_count/review_count)→ `GET /api/collab/activity?projectId=`(两类条目倒序)。
- [ ] **Step 2: 前端真机(须无并行 cargo 后再起 tauri dev)**
① 侧栏出现「团队项目」区+徽章;② 点进项目主页四 tab 都活(概览统计+时间线);③ 「开新讨论」→ 自动建同名本地项目并绑定 → 跳聊天;④ 侧栏团队项目展开可见该对话、「我的项目」区无重复;⑤ 重启 App 绑定仍在。
- [ ] **Step 3: 回归** `cargo test --features server --lib collab` + `npx vue-tsc --noEmit` 全绿。

---

## Self-Review 结论
- 覆盖:侧栏区分=Task 5;项目主页可看情况=Task 4(数据 Task 1/3);对话-项目联动=Task 2/3/4(ensureBound);GitHub 元素=repo 列表徽章/主页四 tab/activity feed/成员条。
- 一致性:字段名 `collab_project_id`/`collabProjectId`、视图键 `collab_project`、`open_count/review_count`、`ActivityItem{kind,actor,task_id,title,detail,at}` 已对齐。
- 取舍:#12 跨引用二期;charter 内容渲染暂缺(collab 项目 charter_path 目前无内容管道);未登录不在侧栏加提示行。
