# 设备联盟 · 真实遥测 + 卡片重设计 + 派任务对话化

> 2026-07-14。用户看着当前「互联」页提出:①三步卡 step3 文字被裁看不见(已修:overflow
> visible + p 色升 text-2 + z-index 分层);②把设备联盟 mockup(每台设备 CPU/内存/磁盘
> 仪表 + 核数 + 传输徽标 + 挂盘/派任务 + 左侧 我的设备/我共享出去的/我能用的/正在发生 子导航)
> 做成我们平台风格,放进「设备与授权」tab;③连接后要有记忆/记录;④派任务 = 左侧新建一个
> 对话,顶部标记目标设备,在里面下指令发给那台机。用户拍板:**仪表要全真(含远端遥测)**、
> **派任务=新建标记对话**。

## 现状(已确认)
- `AdminDevice` 只有 id/user_id/name/node_id/revoked/username/is_host —— **无任何资源字段**。
- 后端**无 sysinfo 依赖**、无 CPU/内存/磁盘采样代码。
- `app` store 有 `createConversation(projectId)` + `setView("chat")` —— 派任务可接真实对话。
- 出站远程盘在 `remotes`(localStorage `polaris.interconnect.remoteSources.v1`);入站设备走
  `collabApi.adminDevices()`(owner 鉴权)。

## Phase 1 —— 本机真实遥测 + 设备卡重设计(先落地可视化)
- 引入 `sysinfo` crate。后端命令 `sys_stats() -> { cpu_pct, mem_used, mem_total, disk_used,
  disk_total, cores }`(desktop 直采本机;server 壳同样暴露供云主机自采)。注册进 lib.rs +
  apihub 双 dispatch。CPU 需两次采样间隔 ~200ms 取增量。
- 前端:`DeviceStats` 类型 + `sysStats()` invoke;`AdminDevice` 加可选 `stats?/cores?`。
- 设备卡重设计(我们平台玻璃风,非 mockup 深色):头部(图标+名+owner+在线点)、传输徽标、
  **三条资源进度条 CPU/内存/磁盘**(本机=真实 sys_stats;远端=「资源待上报」占位,Phase2 填真)、
  核数,底部动作 本机 / 挂它的盘 / 派任务 / 吊销。
- 子导航 filter chips:**我的设备**=adminDevices、**我能用的**=remotes(出站盘)、
  **我共享出去的**=协作授权(先占位计数)、**正在发生**=活动流(先占位/接 activity 端点)。

## Phase 2 —— 远端遥测协议(仪表全真)
- 每台成员 Polaris 定时(~5s)把 sys_stats 通过协作 WS/隧道 **上报**给主机;主机把最新一帧
  存进设备注册表(内存态,带 last_seen)。`adminDevices()` 返回里带 stats。
- 断线/超时 → 标灰「离线」。上报口径如实,不插值不造假(遵码库既有原则)。

## Phase 3 —— 派任务 = 左侧标记对话
- `openDispatch(d)` 改:调 `app.createConversation(projectId)`(取当前/默认 project),
  设标题/顶部标记 `@设备名`,`setView("chat")` 切过去。会话元数据记 `target_device`。
- 真正「指令跨设备下发执行」链路(走 iroh 下发到目标机跑)排 Phase 3.5,先把对话+标记做好。

## Phase 4 —— 手机端上报 + 记忆/记录打磨
- mobile/ 安卓壳:连上后存连接码(只粘一次),开 App 自动重连;并定时上报 sys_stats。
- 「连接后有记录」:入站设备落主机注册表、出站盘落 localStorage —— 均已持久;补一个
  「正在发生」活动流让"连接过"这件事有痕。

## 验收
- 双 flavor(desktop + server)cargo 编译过 + vue-tsc + npm build 过。
- 自包含 exe(`tauri build --debug`)真机:本机卡三条仪表跳动=真实;远端卡上报后=真实;
  派任务→左侧新对话且顶部标 @设备;三步卡 step3 文字可见。
- 全程未 commit(与仓内既有一堆未提交改动同批,待用户点验后统一整理)。
