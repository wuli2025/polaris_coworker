# Polaris 功能树（P1 + P2 接线进展）

2026-10-14 · `refactor/pi-agent`。未迁移节点的行号沿用 `0be46d7` 静态基线；P2 节点以当前文件为准。
原静态基线保留；P2 更新了 chat/headless/壳注册路径，实际检查与未完成项见 [P2 记录](pi-migration/p2-result.md)。
迁移明细见 [调用点](pi-migration/call-sites.md)、[事件契约](pi-migration/agent-events.md)、[超长文件](pi-migration/oversized-files.md)。

## 路径、测试与依赖记法

`K/`=`src-tauri/crates/polaris-kernel/src/`；`F/`=`src-tauri/crates/polaris-fable/src/`；`W/`=`src-tauri/crates/polaris-wiki/src/`。
`C/`=`src-tauri/crates/polaris-collab/src/`；`S/`=`src-tauri/crates/polaris-sandbox/src/`；`H/`=`src-tauri/src/`。
箭头 `A→B` 表示 A 编译依赖或调用 B，桥接回调另行说明，不能把运行时注入误写成 crate 互引。

| 测试代号 | 命令 / 依据 |
| --- | --- |
| K | 在 `src-tauri/` 执行 `cargo test -p polaris-kernel --lib --tests`；CI 单独跑内核以避免 desktop/server 特性合并（`.github/workflows/ci.yml:35,44,46`） |
| F / W | 在 `src-tauri/` 执行 `cargo test -p polaris-fable` / `cargo test -p polaris-wiki`；包定义分别 `src-tauri/crates/polaris-fable/Cargo.toml:9`、`src-tauri/crates/polaris-wiki/Cargo.toml:8`；真实模型/数据评测另有开关，不能算普通测试已覆盖 |
| C | 在 `src-tauri/` 执行 `cargo test -p polaris-collab --features collab-host,collab-net --lib --tests`；特性定义 `src-tauri/crates/polaris-collab/Cargo.toml:58`；不带特性会遇 examples 编译问题（`docs/pi-migration/baseline.md:29`） |
| S | 在 `src-tauri/` 执行 `cargo test -p polaris-sandbox`；依赖 Tauri，需平台开发库（`src-tauri/crates/polaris-sandbox/Cargo.toml:8`） |
| H | 在 `src-tauri/` 执行 `cargo test -p polaris-app --no-default-features --features server --lib <过滤词>`；无桌面形态依据 `src-tauri/Cargo.toml:183`；专家可用过滤词 `expert` |
| UI | 仓库根 `npm run test:unit`、`npm run check:boundaries`、`npm run build`；最后一项包含 vue-tsc 与 Vite 输出（`package.json:8,9,10`） |
| 移动 | `mobile/` 下 `npm run build`；项目切换 E2E 脚本 `mobile/e2e/project-switch.e2e.mjs:1`；需运行中的服务，不能当纯离线单测 |

## 总装树与真实依赖方向

| 模块 → 目录/入口 | 职责与公开接口 | 依赖方向 / 配置 / 测试 |
| --- | --- | --- |
| 桌面壳 → `H/main.rs:1`、`H/lib.rs`、`H/desktop_commands.rs` | Tauri 初始化、命令注册、托盘、退出回收 | app→各业务 crate；`src-tauri/Cargo.toml:161` desktop 特性，`src-tauri/tauri.conf.json:1`；H/UI |
| 服务壳 → `H/server.rs:1,53,84` | HTTP/WS、初始化引擎、运行健康信息 | app→apihub/collab/各引擎；server 特性与 `.env.server.example:1`；H |
| 服务可执行入口 → `src-tauri/crates/polaris-cli/Cargo.toml:23` | 真正的 `polaris-server` bin 在 CLI crate | cli→app（无默认特性、server）；不能按主 Cargo.toml 注释中的 bin 当现存配置（`src-tauri/Cargo.toml:221`）；`cargo check -p polaris-cli`，CI `:85` |
| Vue 桌面/Web → `src/App.vue:1`、`src/tauri.ts:445,1130` | 页壳、invoke/listen 适配、状态与组件 | Vue→桥→Tauri 或 HTTP；`package.json:7` dev，`:8` build；UI |
| 移动端 → `mobile/src/lib/net.ts:315,345`、`mobile/src/lib/chat.ts:208` | HTTP/WS 远程对话、主机/项目切换；不是本地 Agent runtime | mobile→主机 api；`mobile/package.json:6`；移动 |
| HTTP 应用面 → `H/apihub.rs`、`H/apihub/` | invoke 分发、上传/文件、WS、权限校验 | 壳→chat/provider/KB 等；认证与路由不能下沉成模型 SDK 功能；H |
| 引擎桥接装配 → `H/wiring.rs:15,82,98` | 实现 KbBridge、ExpertBridge 并注入内核 | 壳认识 fable/expert；kernel 只认识 trait；无独立磁盘配置；H/K |

实际 crate 依赖应以 manifest 为准：kernel→runtime/watchdog/failover（`src-tauri/crates/polaris-kernel/Cargo.toml:17`），collab→kernel/runtime（`src-tauri/crates/polaris-collab/Cargo.toml:16`）。
fable→kernel/runtime（`F/lib.rs:13`），wiki→fable/kernel/runtime（`W/lib.rs:9`），sandbox→core/Tauri（`src-tauri/crates/polaris-sandbox/Cargo.toml:8`）。
kernel 当前没有 sandbox 编译依赖，虽有旧注释声称 desktop 会启用它；实际保留函数内联容器名（`K/chat/backend/claude_cli/pipeline/spawn.rs`）。

## polaris-kernel → chat

入口 `K/chat/mod.rs:14`，公开类型 `K/chat/types.rs:33,88,108`；测试 K，可用 `chat::` 过滤。

| 子模块 → 入口 | 职责 / 公开接口 | 依赖、配置与迁移边界 |
| --- | --- | --- |
| pipeline → `K/chat/pipeline.rs` | chat 命令、用户消息落库、后台错误终态；调用 AgentBackend | 前端六类事件与 camelCase 字段保持不变 |
| backend → `K/chat/backend/mod.rs`、`config.rs`、`bridge.rs` | AgentBackend / 后端选择 / UI 转发 / delta 收集；Pi 默认，未知值具名错误 | `POLARIS_AGENT_CONFIG` 与环境覆盖；headless 共用接口 |
| claude_cli → `K/chat/backend/claude_cli/pipeline/`、`pool/` | 原 spawn / parser / pool / 提示拼装 / 产物收尾按职责搬移 | 旧工具/模型/池开关保留；原池和管线单测随迁移 |
| session_pool → `K/chat/session_pool.rs` | 原公开路径兼容门面；删除会话、退出转交 backend | 保留 conv 和宿主调用路径 |
| pi → `K/chat/backend/pi/` | JSONL 请求关联、session/turn 路由、有界队列、约 30ms delta 合批、重启/熔断/取消 | 假 daemon 与协议样例测试；shared 已实现；per-account 明确拒绝，尚未实现 |
| 状态 → `K/chat/backend/mod.rs::agent_backend_status` | 后端类型、PID、重启次数、熔断与健康状态 | Tauri/HTTP 已注册；协议 v2 ping 提供版本，握手成功后填 agentdVersion；版本不符熔断 |
| prompt → `K/chat/prompt.rs:66,252,356,1049,1091,1145` | 历史、记忆、检索、批次、目标与动态工作流指令 | →conv/桥返回的数据；当前预算常量 `:22`、`POLARIS_PROMPT_FULL` 逻辑 `:626`；迁到配置/提示资源 |
| types → `K/chat/types.rs:10,33,108` | 四档 PermissionMode，ChatSendArgs，ChatStreamEvent | 权限当前是 CLI 值映射；前端字段由 Rust camelCase 序列化；Pi 权限交互尚待实现 |
| bridges → `K/chat/bridges.rs:48,76,91,96` | KbBridge/ExpertBridge，set_* 一次注入 | 内核不直接依赖 fable 或 expert；装配在 `H/wiring.rs:98`，未注入按 Option 降级 |
| artifacts → `K/chat/artifacts.rs:1`、`K/chat/mod.rs:32` | 产物目录、list/read/write/search/open/reveal；轮末扫描由 pipeline 执行 | →runtime/文件系统；属于业务产物层，应跨后端保留 |
| attach → `K/chat/attach.rs:1`、`K/chat/mod.rs:37` | 图片/文件复制到会话 uploads，转换为可用附件 | →convert/会话路径；不能把附件处理误删为 CLI 专属 |

配置入口：`ChatSendArgs` 包含 permissionMode/useSandbox/skillIds/conversationId/goal/dynamicWorkflow/useKb/batchBuild/batchSize/agentMode/workMode/providerId（`K/chat/types.rs:33`）。
旧后端环境入口移至 `backend/claude_cli/pipeline/{mod,stream,spawn}.rs` 和 `pool/plan.rs`，语义保留。
Pi `useSandbox` 明确报 UnsupportedInput，等待 P3；旧后端原宿主行为保留。Pi 沿用原提示拼装及收尾；默认 daemon 工具面仍只读。

## polaris-kernel → provider

入口 `K/provider/mod.rs:22`；测试 K，可用 `provider::` 过滤。配置由 ProviderDock / AddProviderModal / UsageBoard 访问 `src/tauri.ts:1999` 等桥接接口。

| 子模块 → 入口 | 职责 / 公开接口 | 依赖方向与配置 |
| --- | --- | --- |
| store → `K/provider/store.rs:640,692,1403,1687,1762,1815` | provider_list/switch/save/delete，RouteTarget，Key、路由/联动模式、失败记账 | →failover、integrations::codex_proxy；配置 `~/Polaris/data/providers.json`（`:692`），旧全局 settings 路径 `:1224` |
| 子进程 env → `K/provider/store.rs:1518,1600,1637` | 私有 claude-home、逐会话供应商隔离；CLI onboarding | →std::process::Command；Pi 改 model/runtime 配置，旧机制隔离为 Claude 后端 |
| claude_login → `K/provider/claude_login.rs:24,53,84,183,243` | OAuth 状态、start/finish/poll/cancel，PKCE 回环/手工回贴 | →oauth_loopback、HTTP、文件系统；`~/.claude/.credentials.json`；不是调用 `claude login` |
| codex_login → `K/provider/codex_login.rs:36,56,182,280,412` | installed 探测、回环/设备码登录、poll/cancel、token 保存 | →HTTP/OAuth 工具；`~/.codex/auth.json`；CLI 仅 --version，推理由代理处理 |
| oauth_loopback → `K/provider/oauth_loopback.rs:1,33` | OAuth 共享回调/轮询槽 | 被两种登录实现调用；端口分别见 claude_login `:21`、codex_login `:24` |
| usage → `K/provider/usage.rs:96,165,415` | usage_summary、provider_balance；读取 Claude JSONL、余额查询 | →providers 配置/文件系统/HTTP；`POLARIS_USAGE_SCAN_PAR`；与 meta 估值不同 |
| image_store → `K/provider/image_store.rs:278,304,343,398,410` | 生图配置候选、list/save/delete/switch | 独立 `image-providers.json`（`:106`）；不是聊天表，Pi 迁移保留独立凭据语义 |

## polaris-kernel → integrations / doctor / 公共资源

| 模块 → 入口 | 职责与公开接口 | 依赖方向 / 配置 / 测试 |
| --- | --- | --- |
| integrations/codex_proxy → `K/integrations/codex_proxy.rs:71,1772` | ensure_running、codex_proxy_info；Anthropic 请求转 ChatGPT Responses/OpenAI/Anthropic，SSE 与 token refresh | →provider::route_target/codex_login；本地临时端口、auth.json；K；旧后端适配保留 |
| integrations/feishu → `K/integrations/feishu.rs:270,280,306,726,733,959` | 配置、测试、网关 start/stop/status；事件去重/权限闸→chat_send→回发 | →Node 桥、conv/chat；`feishu.json`（`:94`）、`POLARIS_FEISHU_UNSAFE_HOST_TOOLS`（`:821`）；K |
| 飞书 Node 桥 → `src-tauri/assets/feishu_bridge.mjs:1` | SDK 长连接收消息与回发；Rust 负责 AI 调度 | Rust 在 `K/integrations/feishu.rs:890` 启 node；不是 agentd；随上层 chat 迁移 |
| integrations/wecom → `K/integrations/wecom.rs:217` | wecom_scan_create、企微扫码建机器人 | →HTTP/浏览器；不是另一套 Claude 子进程；K |
| integrations/nas → `K/integrations/nas.rs:210,251,290,303,347` | nas_list/save/forget/connect/disconnect | 系统挂载与路径能力；`nas.json`（`:88`）；与模型后端无直接耦合；K |
| doctor/check + types → `K/doctor/check.rs:15,166`、`K/doctor/types.rs:22,145` | env_check/env_fix_path；工具就绪与安装事件类型 | →probe/path/bundled；当前 ready 依赖 Claude 和 shell；K |
| doctor/probe + path → `K/doctor/probe.rs:337,427`、`K/doctor/path.rs:295` | 解析 CLI、清理子环境、修进程 PATH、Git Bash | →runtime/系统探测；PATH、CLAUDE_CODE_GIT_BASH_PATH；K |
| doctor/install + update → `K/doctor/install.rs:36,58,367,427,555`、`K/doctor/update.rs:102,220` | 安装 Claude/Node/Pwsh/uv、取消、CLI 更新 | →下载源/子进程；日志 env:stream；K；Pi 默认托管对象需替换 |
| doctor/autopilot + verify → `K/doctor/autopilot.rs:62,81`、`K/doctor/verify.rs:1031` | 静默安装；冲突、认证、shell 与深度真实调用校验 | 启动由 `H/lib.rs:201` 接线；Pi/旧后端须区分检测对象；K |
| doctor/bundled + uv_cache → `K/doctor/bundled.rs:29,37`、`K/doctor/uv_cache.rs:1` | 内置运行时定位、uv 缓存维护 | POLARIS_RUNTIME_DIR / 安装资源目录；通用工具保留；K |
| headless → `K/headless.rs` | run_claude_readonly(_timeout)、extract_balanced_json | →AgentBackend/collect；被 wiki/fable 间接调用；保留公开函数名和 JSON 提取；K |
| conv → `K/conv.rs:91,496,538,753,959` | 项目/会话 CRUD、消息历史、绑定 KB scope/工作目录 | →文件系统与 session_pool 回收；`~/Polaris/data/state.json`（`:95`）；K |
| claude_md → `K/claude_md.rs:65,76,175,185,218` | 项目/KB 上下文 CRUD、渲染 | →conv/KB 桥；项目与 KB 的 CLAUDE.md；Pi 加 AGENTS.md 兼容规则；K |
| skills → `K/skills/mod.rs:16,23`、`K/skills/store.rs:195` | 内置目录/市场、安装创建、意图匹配、模板分发、find | →磁盘 `~/Polaris/skills` 与编译期模板（mod `:3`）；→chat 注入；K |
| convert → `K/convert.rs:1` | Office/PDF 等转换，供附件/KB 使用 | 内核公共转换库；无 CLI Agent 协议依赖；K |

## 宿主 expert* / persona

入口 `H/expert.rs:24`，公开命令在 `H/expert.rs:273,339,407,547,560,617,640,855`；测试 H（expert/persona）。

| 子模块 | 职责、公开接口与依赖 |
| --- | --- |
| 能力卡目录 → `H/expert/expert_groups.rs:47` | build_experts 构建卡片；信号、关键词、能力、组别、模型提示等字段定义于 `H/expert.rs:24`；元数据不等于真正启动模型 |
| 模板 → `H/expert/expert_docs.rs:22`、`H/templates/experts/` | 编译期嵌入专家提示；卡片构建完整导出文本；include_dir 依赖见 `src-tauri/Cargo.toml:62` |
| 团队 → `H/expert/teams.rs:55,142` | all_teams、build_team_doc，按领队/成员组装编排提示 |
| 路由 → `H/expert.rs:325,339,499,703` | 信号与关键词两路分数经加权 RRF 融合；expert_route、route_block；不是向量召回服务 |
| 召集 → `H/expert.rs:230,266,760` | expert_team_spawn 选最多五人并维护内存状态，team_block 返回提示段；没有多个子进程/独立会话 |
| 应用/导出 → `H/expert.rs:407,448,560,617,640` | 手动应用写项目 CLAUDE.md 并记 persona_id；导出返回文本；→conv/文件系统 |
| persona → `H/persona.rs:170,177` | 预设人格列举、应用，复用上下文/KB scope；同样写项目 CLAUDE.md |
| avatars → `H/expert/avatars.rs:51,58` | 内嵌头像 data URL 与槽位；与 Agent 后端无关 |
| 对话接入 → `K/chat/backend/claude_cli/pipeline/prepare.rs`、`H/wiring.rs:82` | agentMode 选择 expert-team/auto-match 等分支；经 ExpertBridge 注入，不在每轮自动改人格文件 |

配置入口：会话 `agentMode`（`K/chat/types.rs:66`）、项目 persona_id/KB scope（`K/conv.rs:485`）、编译期卡片/模板。
`experts.enabled=false` 是规划目标，当前不是已有全局开关（`docs/plans/2026-10-14-pi-refactor.md:39`；实际入口 `K/chat/backend/claude_cli/pipeline/prepare.rs`）。
迁移保留能力卡/路由，替换文件写入与提示加载方式；不要把专家列表状态当成真实子 Agent 执行证据。

## polaris-collab → auth / authority / mesh / tunnel / gateway

模块树 `C/collab/mod.rs:4`；测试 C。共用 SQLite `~/Polaris/data/collab.db`，`POLARIS_COLLAB_DB` 可覆盖（`C/collab/db.rs:28`）。
账号认证、设备准入、传输、应用权限是不同层；不要把“登录成功”“入网成功”“有隧道”“能执行工具”合并成一个状态。

| 模块 → 入口 | 职责 / 公开接口 | 依赖方向 / 配置 |
| --- | --- | --- |
| auth → `C/collab/auth.rs:416,519,709,881,981` | 账号密码验证、session 签发/检查/注销、authority_login；角色管理 `:1095` | →db/身份数据；argon2 依赖 `src-tauri/crates/polaris-collab/Cargo.toml:35`；session 缓存与吊销逻辑 auth `:69,76` |
| authority → `C/collab/authority.rs:56,163,187,268,341` | Local / Authority / Delegated 三模式；Ed25519 断言签验、公钥 pin、转发登录 | →auth/db/HTTP；POLARIS_ACCOUNT_AUTHORITY、POLARIS_ACCOUNT_AUTHORITY_URL、POLARIS_ACCOUNT_KEY（`:57,63,86`）；断言 TTL=300s（`:31`），不是长期 session |
| authority 准入 → `C/collab/authority.rs:557,766,775,849` | upsert_member、login_with_assertion、local 登录、邀请票据加入 | →本机成员资格/grants；权威 uid 与本机 users.id 不等同（`:18`）；P6 保留跨账号邀请权限边界 |
| identity → `C/collab/identity.rs:32,123,180,256,565,645` | 自动生成 host.key、连接证明、访问码/邀请、设备注册吊销、节点放行 | →db/密钥；POLARIS_HOST_KEY（`:19`）；密钥已自动生成，不能写成必须手工创建 |
| grants → `C/collab/grants.rs:131,167,217,256,274,356,383` | peer_grants 持久信任、权限级别、文件/执行门禁 | →db/本机设备身份；同账号设备已有自动登记，除首台外有 24h 冷静期，实际文件只读、执行 ask（`:28,65,76,181`） |
| mesh（权威目录）→ `C/collab/mesh.rs:79,106,126,198,221` | enroll、resolve_key、peers、announce、revoke；同 uid 设备目录 | →db/sha256；meshKey 为长期随机凭据，库只存 hash（`:12,87`）；不是 device.key，也不是设备证书 |
| tunnel → `C/collab/tunnel.rs:204,256,374,539,575,743` | 主机监听、共享出站 Endpoint、TCP↔iroh 双向流、connect/disconnect/status、relay 配置 | →identity/iroh；collab-net；POLARIS_DEVICE_KEY、POLARIS_RELAYS、POLARIS_TUNNEL_UPSTREAM、POLARIS_TUNNEL_STRICT（`:192,317,92,421`） |
| relay 配置 → `C/collab/tunnel.rs:256,317,334,343` | apply_relay_config、自定义 relay 列表、mDNS | 默认 n0 relay（`:12`）；HTTP 配置入口 `C/collab/http.rs:3460,4667`；运行中的 Endpoint 不会就地重建（tunnel `:255`），不是已部署自有 relay 的证据 |
| gateway → `C/collab/gateway.rs:15,109,167,284,368` | 网关注册主机、单 Endpoint 连多个主机、HTTP/WS `/h/:id` 反代 | →tunnel 协议/CollabState；collab-host+collab-net；POLARIS_GATEWAY_KEY（`:69`），独立 gateway.key；不是 iroh-relay 服务 |
| http → `C/collab/http.rs:146,176,4506` | resolve_auth、auth_ctx、collab_router；协作/账号/设备/网关路由，双壳共用 | →上述 domain；不包含所有业务 invoke 分发，后者在 `H/apihub.rs:2218` |

## 互联客户端与协作业务分支

同账号准入已会调用 auto_grant_self，已有契约不被重连覆盖（`C/collab/authority.rs:820,824`；`C/collab/grants.rs:177`）。P6 应调整登录入口与默认信任/冷静期策略，不能按“现有系统完全没有自动信任”重做。

| 模块 → 入口 | 职责 / 公开接口 | 配置 / 依赖 / 测试 |
| --- | --- | --- |
| 壳 mesh 客户端 → `H/mesh.rs:44,53,761,810,866,932,970` | account_login_code、mesh_join、登录后准入/enroll/后台对账；不是 collab crate 的目录表 | →authority/mesh HTTP/tunnel/挂盘；mesh_url/key/uid 存 meta；桌面当前走邮箱验证码（`:967`）；H/C |
| 设备操作 → `H/mesh.rs:1136,1166,1180,1295,1347` | peer_grant_set、peer_trust/revoke、device_code、connect_by_code | →grants/identity；P6 改默认信任体验，保留显式收紧能力；H |
| 桌面 hosting → `H/hosting.rs:111,338,441,470,492,558` | 内嵌主机 start/status/stop、自启动、LAN 开关，桥接事件 | →collab router+apihub；collab_host.json、POLARIS_HOST_ALLOW_LAN、POLARIS_AUTH_TOKEN（`:58,102,128`）；H |
| 账号前端 → `src/features/auth/AccountCenterModal.vue:1`、`src/features/auth/authorityTransport.test.ts:1` | 账号中心、权威传输选择 | →壳 mesh/认证 API；UI，有已有传输策略单测 |
| 互联前端 → `src/features/interconnect/InterconnectView.vue:1` | 设备网、授权/连接码、能力状态与操作入口 | →mesh/hosting 桥；UI；P6 重点 |
| 协作项目/团队/任务 → `C/collab/mod.rs:39,40,41` | projects/teams/tasks；账号权限和任务状态 | →db/auth；`src/features/collab/api.ts:1`、stores/collab.ts `:1` 为 UI API 与状态入口；C/UI |
| 协作对话/负责人 → `C/collab/mod.rs:12,36,37` | chat、lead、lead_ai 负责协作者/负责人沟通和任务编排 | 与 kernel/chat 的模型会话不等同；权限由确定性授权表决定（mod `:3`）；C |
| 独立负责人模型 → `C/collab/lead_ai.rs:41,49,149,193,237,276` | load/save_cfg、ai_decompose/review/task_reply/fuse | OpenAI 兼容 HTTP，配置 `lead_model.json` / POLARIS_LEAD_MODEL_CFG（`:5,28`）；不是 headless/Claude CLI；C |
| 工作集/检查/合并 → `C/collab/mod.rs:14,18,38,42` | workset、checks、gitea、mergectl：工作树、检查链、代码托管、合并闸门 | →skills/运行时/仓库工具；不应被 Pi 接管权限裁决；C |
| 文件服务/镜像 → `C/collab/mod.rs:4,21` | fsface 远程文件浏览；account_store 加密账号镜像 | fsface 按开放根限制；POLARIS_FS_ROOTS（`H/hosting.rs:202`）；account_store export/restore（`C/collab/account_store.rs:26,136`）；C |
| 壳远程数据面 → `H/fsmount.rs:1`、`H/exec.rs:308`、`H/appproxy.rs:1`、`H/beam.rs:1` | 挂盘、执行、应用代理、同步 | →collab/tunnel/应用面；属于互联数据面，不能等同 Agent CLI；H |

## polaris-sandbox

入口 `S/lib.rs:16`；测试 S。`polaris-core::KbLocator` 由宿主注入 KB 根，sandbox 不依赖 fable（`S/commands.rs:21,209`；`src-tauri/crates/polaris-core/src/lib.rs:44`）。

| 分支 → 入口 | 职责与公开接口 | 配置与迁移处理 |
| --- | --- | --- |
| Docker → `S/commands.rs:32,61,136,178,259,277` | init/status/build_image/start/stop/exec，封装 docker CLI | 固定 image/container（`:27`），材料写 `~/Polaris/sandbox/Dockerfile`（`:33`）；工作目录/KB 挂载（`:205`） |
| 镜像 → `S/templates/Dockerfile.sandbox:23` | Alpine 安装 Claude 原生包；不是 Node agentd 镜像 | P3 调整工具环境；旧 Claude 镜像可配置保留 |
| E2B/Cube → `S/e2b.rs:23,59,75,80,89` | CubeConfig、配置读写、curl 状态探测 | `~/Polaris/sandbox/cube-sandbox.json`，backend/endpoint/apiKey；无 Agent exec 路由实现（`:13,130`） |
| 对话连接点 → `K/chat/backend/claude_cli/pipeline/{process,spawn}.rs` | useSandbox 跳过池后仍在宿主执行；容器启动函数未调用 | P3 需新工具执行桥；不得把已有 UI 状态当作隔离成立 |
| UI → `src/features/sandbox/components/SandboxStatus.vue:1` | 构建、启动、状态及后端配置面板 | →sandbox/cube 命令；UI；与真实 chat 路由一起验收 |

## 其他引擎与工作台功能

| 模块 → 目录/入口 | 职责 / 公开接口 | 依赖、配置与测试 |
| --- | --- | --- |
| fable / KB → `F/lib.rs:7,8`、`F/fable/mod.rs:100` | 盘点/索引/混合检索/文件中心，KB 访问/导入/图谱/维护 | →kernel/runtime；POLARIS_FABLE_DB；UI FileCenter/WikiBrowse/KnowledgeGraph；F/UI |
| AI 检索与知识抽取 → `F/fable/retrieve/ai.rs:205`、`F/fable/ontology.rs:476` | 查询扩写、本体三元组，复用 headless | →kernel；保持只读推理与 Rust 校验边界；F |
| 文件中心模型 → `F/fable/files/llm.rs:243,262,891,997`、`F/fable/files/profile.rs:404` | 独立 HTTP 分类模型或聊天模型 fallback；画像、标题、归类 | →headless/独立模型配置；F；真实模型评测开关在 files/tests.rs `:254` |
| echo → `F/fable/echo.rs:125,300,660,993` | 做梦/对话记忆、资料分析与晨报 | →conv/headless/KB；业务事件 echo:dream；F/UI |
| wiki → `W/lib.rs:6,9`、`W/wiki/compile.rs:105` | wiki 知识网编译、进度事件与重扫索引 | →fable/kernel/runtime；当前独立 Claude 进程；W |
| forge → `src-tauri/crates/polaris-forge/src/lib.rs:8,11` | 演示/PPT、截图/视频/TTS/Figma 桥 | →runtime；与 Agent 工具调用者分离；CI `.github/workflows/ci.yml:81` 的 forge 测试命令 |
| forge CLI → `src-tauri/crates/polaris-cli/src/main.rs:1` | 脚本可调用的渲染引擎，polaris-forge | →app server 形态；不会因 Agent 换 Pi 而失去用途；CI `:85` 检查 |
| 语音 → `H/voice/mod.rs:534,569`、`H/voice/asr.rs:64`、`H/voice/live.rs:96` | 本地识别、热词纠错、HTTP 润色、录音与输入 | →sense/provider 借 Key；voice.json/voice_lexicon.json（mod `:6`）；voice-asr / voice-live 特性（`src-tauri/Cargo.toml:190`）；H，ASR 冒烟命令见 asr.rs `:98` |
| 媒体账号/项目运行 → `H/accounts.rs:136,144`、`H/project.rs:60` | 浏览器 profile 状态/解绑、可运行产物启动 | 非 Claude/Codex OAuth；保留业务接口；H |
| 自动化/批次/工作流 → `src/stores/automation.ts:323`、`src/stores/longtask.ts:1`、`src/stores/workflows.ts:248` | 调度 chat，多轮续跑及 prompt 模板 | →chat store/tauri 桥；UI；Pi 保留 done 对后续批次的驱动 |
| 协议 → `src-tauri/crates/polaris-protocol/src/lib.rs:18,31` | InvokeRequest 与 Args 参数访问/严格检查 | 此 crate 仍是宿主 invoke 协议；agentd 镜像类型在 kernel/backend/pi/protocol.rs；`cargo test -p polaris-protocol` |
| runtime → `src-tauri/crates/polaris-runtime/src/lib.rs:9` | 路径、HTTP、子进程、host 广播 shim | 最底层无业务依赖；`cargo test -p polaris-runtime` |
| core → `src-tauri/crates/polaris-core/src/lib.rs:24,44` | SandboxStatus / KbLocator 等共享契约 | sandbox 通过它反转 KB 依赖；`cargo test -p polaris-core` |
| watchdog / failover → `src-tauri/crates/polaris-watchdog/src/lib.rs:1`、`src-tauri/crates/polaris-failover/src/lib.rs:1` | 超时状态机、供应商失败窗口 | kernel 编译依赖见 `src-tauri/crates/polaris-kernel/Cargo.toml:19`；分别 `cargo test -p polaris-watchdog` / `-p polaris-failover` |
| forge-codec → `src-tauri/crates/forge-codec/src/lib.rs:1` | 原生编解码能力 | app 可选 native-codec；默认未开（`src-tauri/Cargo.toml:203`）；独立 `cargo test -p polaris-forge-codec` |

## agentd（P1 / P1-R1）

入口 `agentd/src/main.ts`；JSONL 协议 `agentd/src/protocol/schema.ts` / `events.ts`；配置入口 `agentd/config/agent.default.json`、`--config`、`config.apply`。

| 分支 → 入口 | 职责与行为 | 配置与验证 |
| --- | --- | --- |
| 调度 → `agentd/src/daemon.ts` | 按 sessionId 串行、跨会话并发；ping/models.list 不排队；退出等待在途命令 | `agentd/test/concurrency.test.ts` |
| 轮次 → `agentd/src/sessions/registry.ts` | 任意 SDK 事件重置空闲计时；硬上限独立计时 | turnIdleTimeoutMs 默认 180000；turnMaxMs 默认 0（不限）；`timeouts.test.ts` |
| 会话设置 → `agentd/src/sessions/create.ts` | 默认启用重试与压缩；重试次数和退避传给 SettingsManager | retry.maxRetries/baseDelayMs/maxAgentDelayMs；`config-review.test.ts` |
| 错误 → `agentd/src/events/model-error.ts`、`config/validate.ts` | 模型错误按 SDK diagnostics 分类，固定中文模板；配置错误仅返回字段路径 | `model-errors.test.ts`、`config-review.test.ts` |
| 运行时 → `agentd/src/models/runtime.ts` | config.apply 重建运行时，响应 reopenRequired 列出需关闭重开的已有会话 | SDK 无公开运行时替换接口；`runtime.test.ts`；详见 [R1 实现说明](agentd-review-r1.md) |
| 工具 → `agentd/config/agent.default.json` | 默认档与 readonly 为 read/grep/find/ls；workspace 加 edit/write，full 再加 bash；按会话选择配置中的名单 | P2.5：Deny→readonly，其余→workspace；这是工具选择，P3 细化权限；`config-review.test.ts`、`protocol-extension.test.ts` |

验证命令：`cd agentd && npm run typecheck && npm test`；逐项证据与环境限制见 [R1 实现说明](agentd-review-r1.md)。

## P2.5 协议 v2（执行报告，验收仍 BLOCKED）

唯一协议源：`agentd/src/protocol/schema.ts` / `events.ts`；Rust 镜像 `backend/pi/protocol.rs`。
两端必须一并升级；旧版/缺失 protocolVersion 的 ping 会触发 AgentdProtocolMismatch 并立即熔断。

| 入口 | 行为与依赖 | 配置、迁移与验证 |
| --- | --- | --- |
| `agentd/src/sessions/create.ts`、`options.ts`、`registry.ts` | session.open 可选 model/thinkingLevel/toolProfile；session.configure 仅空闲可用，SDK setModel/setThinkingLevel/setActiveToolsByName | defaultModel/thinkingLevel/defaultToolProfile；toolProfiles 名单由配置管理，旧 tools 只映射 default 档；`agentd/test/protocol-extension.test.ts` |
| `agentd/src/sessions/attachments.ts` | prompt.attachments 图片 base64 校验，经 SDK prompt.images 传入模型 | attachments.maxCount=4、maxBytes=262144、maxTotalBytes=524288，PNG/JPEG/GIF/WebP；faux 模型验证真实消息 |
| `agentd/src/protocol/version.ts`、`backend/pi/supervisor.rs` | ping 返回 agentd/npm 实际版本、Pi SDK VERSION、protocolVersion=2；握手成功前阻止业务请求 | 沿用 requestTimeoutMs/channelCapacity；版本错误立即熔断；`pi/extension_tests.rs` 验证仅发出 ping |
| `backend/bridge.rs`、`pi/input.rs`、`pi/sessions.rs` | 每轮模型及权限进 open/configure；已有会话每轮重新配置；TurnRequest 图片路径读为 MIME/base64；公开 PiBackend.configure_session 支持思考档位 | 复用 POLARIS_WORK_MODEL/POLARIS_FAST_MODEL，通过 models.list 唯一匹配 provider，歧义具名拒绝；Deny→readonly，其余→workspace |
| `tests/fixtures/protocol-samples.jsonl`、`protocol-schema.sha256` | 双端样例与 TS schema 哈希锁定，新增 configure/图片/版本样例 | TS 校验请求，Rust 逐条解析及往返；backend 定向测试含 R1c 回归 |

入口限制：ChatSendArgs 没有结构化图片和思考档位字段，未越界修改 `chat/types.rs`；UI 路径文本保持原行为，不能声称图片 UI 已端到端接通。
默认值、类型校验、切换示例、SDK d.ts 出处、命令真实结果与文件行数见 [P2.5 执行报告](pi-migration/p25-result.md)；无新依赖、未提交。

## P1/P2 审查修复（R1，部分执行）

| 分支 → 入口 | 行为、依赖与配置 | 迁移与验证 |
| --- | --- | --- |
| Key 脱敏 → `agentd/src/models/credentials.ts`、`runtime.ts` | Redactor.add 登记原文、URL 编码、base64 和 JSON 转义；Node 内建函数，无新依赖 | startup/apply 的 Logger 与工具事件测试：`agentd/test/credentials-review.test.ts`；完整字符串匹配，不新增跨 delta 缓存 |
| Route 生命周期/首错 → `backend/pi/routes.rs`、`supervisor.rs` | Drop 清理 active/reservation，State Drop 清空孤立 active；首错保留、次错仅日志；依赖既有 Translator/Mailbox | Route 单测及 `pi/tests.rs` 覆盖熔断/退出后 config.apply 不再 ConfigBusy；无配置迁移 |
| 写入诊断 → `backend/pi/process.rs`、`client.rs`、`supervisor.rs` | WriterBackpressure/WriterClosed 分离；lastError 和日志保留根因；沿用 requestTimeoutMs/supervisorTickMs/channelCapacity | 满队列、关闭管道和真实 stdin 阻塞测试；默认值、校验与切换示例见执行报告 |
| 前端契约 → `backend/pi/translate.rs` | 注释说明丢弃 reasoning/usage，后续以可选字段扩展 | 无事件行为变化；既有翻译测试通过 |
| 超时迁移 P2-5 / R1b → `backend/config.rs`、`bridge.rs`、`pi/mod.rs`、`pi/routes.rs`、`pi/supervisor.rs` | 按有效轮次原始事件刷新空闲计时（含 reasoning/usage）；bridge 轮询等待终态；依赖既有 Control/Translator/Mailbox，无新依赖 | turnIdleTimeoutMs 默认 240000（大于 agentd 180000，留出自行收尾时间），turnMaxMs 默认 0 不限；旧 turnTimeoutMs 具名拒绝；迁移示例、行为测试与实际验证见 [R1b 报告](pi-migration/r1b-idle-timeout.md) |
| 丢弃收尾 R1c → `backend/pi/{routes,supervisor}.rs`、`backend/claude_cli/completion.rs`、`claude_cli/pool/reader.rs` | Route Drop 保证未结束 mailbox 收到 AgentdSupervisorExited + done；State Drop 更新故障状态；legacy stdout/常驻 job Drop 同样兜底，reader 退出清活跃标记 | 依赖既有 Mailbox/AgentBackend/Control，无新依赖或配置迁移；沿用后端和常驻模式开关；panic/通道关闭/背压/正常终态测试、命令及例外见 [R1c 报告](pi-migration/r1c-drop-completion.md) |

Rust 入口路径以 `src-tauri/crates/polaris-kernel/src/chat/` 为前缀。
实际命令、结果、基线/TCP 失败列表与边界阻塞见 [R1 执行报告](pi-migration/r1-review-fixes.md)；R1 不声明完整验收通过。

## 后续边界与待确认

- P2 shared Pi 接线已落地但未完成验收：per-account OS/账号隔离未实现；完整内核测试有 6 项额外的沙箱端口失败。配置、限制与结果见 [P2 记录](pi-migration/p2-result.md)。
- U1–U4：OAuth/账本合并/上下文优先级/真实沙箱范围，详见 [调用点待确认](pi-migration/call-sites.md#待确认)。
- U5–U7：取消终态、手机 error 语义、新事件承载，详见 [事件待确认](pi-migration/agent-events.md#后续契约测试清单与待确认)。
- U8（待确认）：P6 登录后 relay 自动下发需要核对桌面与手机接线；已有 apply_relay_config 只证明接收能力（`C/collab/tunnel.rs:256`）；找到的生产调用是 owner HTTP 配置路由（`C/collab/http.rs:3460`），登录流程没有直接调用它，自动下发与 Endpoint 更新策略留 P6。
- [baseline.md](pi-migration/baseline.md) 保留原 4 项失败；P2 实测单独记录，不扩大基线豁免范围。
