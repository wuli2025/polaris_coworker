# Polaris 功能树（P1–P3 接线进展）

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
kernel 没有 sandbox 编译依赖；P3 通过壳注入 SandboxExecutor，使用既有受控容器，原 spawn_in_sandbox 死代码已删除。

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
Pi `useSandbox` 在 P3 路由 bash/write/edit 到宿主容器桥；不可用或挂载暴露凭据时拒绝。legacy 的沙箱请求明确拒绝。默认权限 manual，工具可见性和逐次授权独立。

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

入口 `S/lib.rs`；测试 S。`polaris-core::KbLocator` 由宿主注入 KB 根，sandbox 不依赖 fable。
P3b 已接入桌面路由校验、请求级专家选择和项目迁移 UI；服务端沙箱策略依赖仍待确认，完整边界见 [P3b 执行报告](pi-migration/p3b-result.md)。

| 子模块 → 入口 | 职责 / 公开接口 | 配置、迁移与验证 |
| --- | --- | --- |
| Docker → `S/commands.rs` | status/build/start/stop/exec；start 要求 projectDir，缺失报 SANDBOX_PROJECT_REQUIRED | 保留固定镜像/容器名；status/诊断 exec 检查当前挂载与安全属性；S |
| 策略 → `S/config.rs`、`S/policy.rs` | Config::load/resolve_mounts/run_args/validate_container；只允许项目 RW + KB RO | POLARIS_SANDBOX_CONFIG；schema=`polaris-sandbox/sandbox.schema.json`；兜底保护路径只能追加；默认 4GiB/2CPU、drop ALL/no-new-privileges |
| 生命周期 → `S/lifecycle.rs` | start/inspect；安全同项目复用，工作目录变化按 reject/rebuild 配置处理 | 默认 reject；旧宽挂载总是显式迁移；rebuild:true 停止并改名保留旧容器，无自动删除；R4 沙箱 17 项测试通过 |
| 镜像 → `S/templates/Dockerfile.sandbox` | Alpine + Claude 原生包；不是 Node agentd 镜像 | 注释同步最小挂载与 cap-drop；未构建或启动真实 Docker |
| E2B/Cube → `S/e2b.rs` | CubeConfig、配置读写、curl 状态探测 | 原有行为保留；无 Agent exec 路由实现 |
| 对话连接点 → `K/chat/backend/{host_tools,docker_tools}.rs`、`H/wiring.rs` | SandboxPolicy 公开端口；桌面每次执行前检查完整 inspect，委托 S::Config::validate_container | 同一份追加保护名单、目标目录/目的地/RW、CPU/内存/cap-drop/NNP；服务端未安装 executor，显式 SANDBOX_UNAVAILABLE，不回退旧校验 |
| UI → `src/features/sandbox/{api.ts,project.ts,components/SandboxProject.vue}` | 默认当前项目已绑定目录；选择/输入绝对路径，确认启动后绑定；异常显示保留旧容器并一键重建 | SandboxStatus 引入组件，净减 12 行；空路径不发起请求；project.test.ts；未做真实 Docker/浏览器点击验收 |

默认值、配置切换示例、迁移命令、完整失败/豁免项与修改清单见 [P3b 执行报告](pi-migration/p3b-result.md)。

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
| 工具 → `agentd/config/agent.default.json` | 默认档/readonly 为只读及联网/子任务；workspace/full 加 edit/write/bash | P3：Deny→readonly，其余→workspace，另发完整权限档逐次检查；`permissions.test.ts`、`protocol-extension.test.ts` |

验证命令：`cd agentd && npm run typecheck && npm test`；逐项证据与环境限制见 [R1 实现说明](agentd-review-r1.md)。

## P2.5 协议 v2（执行报告，验收仍 BLOCKED）

唯一协议源：`agentd/src/protocol/schema.ts` / `events.ts`；Rust 镜像 `backend/pi/protocol.rs`。
两端必须一并升级；旧版/缺失 protocolVersion 的 ping 会触发 AgentdProtocolMismatch 并立即熔断。

| 入口 | 行为与依赖 | 配置、迁移与验证 |
| --- | --- | --- |
| `agentd/src/sessions/create.ts`、`options.ts`、`registry.ts` | session.open 可选 model/thinkingLevel/toolProfile；session.configure 仅空闲可用，SDK setModel/setThinkingLevel/setActiveToolsByName | defaultModel/thinkingLevel/defaultToolProfile；toolProfiles 名单由配置管理，旧 tools 只映射 default 档；`agentd/test/protocol-extension.test.ts` |
| `agentd/src/sessions/attachments.ts` | prompt.attachments 图片 base64 校验，经 SDK prompt.images 传入模型 | attachments.maxCount=4、maxBytes=262144、maxTotalBytes=524288，PNG/JPEG/GIF/WebP；faux 模型验证真实消息 |
| `agentd/src/protocol/version.ts`、`backend/pi/supervisor.rs` | ping 返回 agentd/npm 实际版本、Pi SDK VERSION、protocolVersion=4（R3 升级）；握手成功前阻止业务请求 | 沿用 requestTimeoutMs/channelCapacity；版本错误立即熔断；`pi/extension_tests.rs` 验证仅发出 ping |
| `backend/bridge.rs`、`pi/input.rs`、`pi/sessions.rs` | 每轮模型、权限及思考档位进 open/configure；已有会话每轮重新配置；TurnRequest 携带已校验图片内容；公开 PiBackend.configure_session 支持思考档位 | 复用 POLARIS_WORK_MODEL/POLARIS_FAST_MODEL，通过 models.list 唯一匹配 provider，歧义具名拒绝；Deny→readonly，其余→workspace |
| `tests/fixtures/protocol-samples.jsonl`、`protocol-schema.sha256` | 双端样例与 TS schema 哈希锁定，新增 configure/图片/版本样例 | TS 校验请求，Rust 逐条解析及往返；backend 定向测试含 R1c 回归 |

P2.5 当时缺失的 ChatSendArgs 图片/思考档位入口由下述 P2.5b 接通；P2.5 原报告保留为历史执行记录。
默认值、类型校验、切换示例、SDK d.ts 出处、命令真实结果与文件行数见 [P2.5 执行报告](pi-migration/p25-result.md)；无新依赖、未提交。

### P2.5b 聊天图片与思考档位入口（已接线，待主控复核）

入口：`src/stores/chat.ts` → `src/lib/chatImages.ts::uploadedImagePaths` → `src/tauri.ts::chat.send`。
成功上传的 image 附件传入 camelCase `imagePaths`；prompt 路径文本保持原样，旧前端省略字段可用。
`K/chat/types.rs::ChatSendArgs` 增加 serde 默认的 image_paths/thinking_level；没有现成档位选择器，不新增 UI。
`backend/bridge.rs` 经 `attach::uploads_dir` 定位当前会话 uploads，再调用公开 `backend/chat_input.rs::prepare`。
该接口复用 `images.rs::load`，拒绝相对路径、`..`、规范化越界及符号链接逃逸，验证普通文件、数量、字节和 MIME 后编码。
已知限制（R2 / 审查 #2）：规范化后、打开前，上级目录仍可能被替换为符号链接；攻击者须有会话 uploads 目录的本地写权限。
远程协作者只能提交路径字符串，不能写本机文件系统，此情形不在当前威胁模型内；fd 的 `file.metadata()` 本身使用 fstat。
若将来 uploads 对他人可写，应改用 Linux `openat2(RESOLVE_BENEATH)`；本轮仅记录限制，不修改附件代码。
`TurnRequest.attachments` 现为 `Vec<protocol::Attachment>`；Pi 直接下发内容，不再重新打开路径。
`thinking_level` 校验为 off/minimal/low/medium/high/xhigh，非法字符串报 THINKING_LEVEL_INVALID；Pi 每轮 open/configure 下发。
缺省沿用 daemon 默认/当前档位；Claude CLI 保留 prompt/Read 路径行为，不增加思考档位映射。
依赖已有 std/base64/anyhow/serde/libc 与 Pi 协议；无新依赖。headless/飞书仅补 None 默认值。
宿主配置 `POLARIS_AGENT_CONFIG` 的 `agentd.attachments` 默认与 P2.5 一致：4 张、262144/524288 字节、PNG/JPEG/GIF/WebP；
整数/正字节数/白名单均校验，maxCount=0 禁用；旧配置无需迁移。提高额度需同步 daemon attachments 和两端 maxLineBytes。
迁移：Rust TurnRequest 路径调用方应先经加载器转为 Attachment，并显式提供 thinking_level；IPC 仍使用可选 camelCase 字段。
验证：`images/tests.rs`、`chat_input/tests.rs`、`pi/extension_tests.rs` 覆盖边界及 ChatSendArgs→TurnRequest→协议；前端 `chatImages.test.ts`。
两壳 check、vue-tsc、58 个 backend 测试通过；kernel 全量 158 通过、10 项许可例外失败、2 忽略，另补跑 4 个集成测试通过。
前端两旧文件各净增 2 行；既有超长文件按例外保留。配置/调用示例、完整命令和日志见 [P2.5b 报告](pi-migration/p25b-result.md)。
Pi 安装版 read 工具支持图片内容块，可为仅有路径的 prompt 提供按需兜底；需要模型实际调用工具，不是自动附图。

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

## R2 审查修复（执行报告，完整验证 BLOCKED）

仅实现 [审查处置](reviews/P25-round1-triage.md) 的 #3/#4；#2 限制见上方附件节点，驳回项未改代码。

| 入口 | 行为与依赖 | 配置、迁移与验证 |
| --- | --- | --- |
| `K/chat/backend/pi/supervisor.rs`、`client.rs` | State Drop 显式回复全部 deferred/pending；回复通道关闭报 AgentdSupervisorExited，真实等待超时报 AgentdRequestTimeout；保留 actor 显式错误 | 依赖既有 mpsc；requestTimeoutMs=15000、supervisorTickMs=10、channelCapacity=64，均为正整数；无新增配置；`supervisor_tests.rs`、client 内单测及既有 Pi 回归 |
| `agentd/src/sessions/options.ts`、`registry.ts` | configure 预检字段类型/思考枚举、模型、工具档及模型输入能力；失败按原模型→思考→工具回滚；toolProfile 仍只在成功后更新 | 依赖既有 SDK/TypeBox；沿用 defaultModel/thinkingLevel/defaultToolProfile/toolProfiles，缺省保持原行为；无需配置/协议迁移；`agentd/test/configure-transaction.test.ts` 与 `protocol-extension.test.ts` |

配置切换示例、回滚边界、实际命令、失败清单及未运行项见 [R2 执行报告](pi-migration/r2-review-fixes.md)。
无新依赖、未提交；最终完成状态由 Claude 复核决定。

## 后续边界与待确认

- P2 shared Pi 接线已落地但未完成验收：per-account OS/账号隔离未实现；完整内核测试有 6 项额外的沙箱端口失败。配置、限制与结果见 [P2 记录](pi-migration/p2-result.md)。
- U1–U4：OAuth/账本合并/上下文优先级/真实沙箱范围，详见 [调用点待确认](pi-migration/call-sites.md#待确认)。
- U5–U7：取消终态、手机 error 语义、新事件承载，详见 [事件待确认](pi-migration/agent-events.md#后续契约测试清单与待确认)。
- U8（待确认）：P6 登录后 relay 自动下发需要核对桌面与手机接线；已有 apply_relay_config 只证明接收能力（`C/collab/tunnel.rs:256`）；找到的生产调用是 owner HTTP 配置路由（`C/collab/http.rs:3460`），登录流程没有直接调用它，自动下发与 Endpoint 更新策略留 P6。
- [baseline.md](pi-migration/baseline.md) 保留原 4 项失败；P2 实测单独记录，不扩大基线豁免范围。


## P3 权限、工具与资源（实现待验收）

| 入口 | 公开接口及依赖 | 配置、迁移、验证 |
| --- | --- | --- |
| `agentd/src/extensions/permissions.ts` | Pi tool_call 强制闸；permission.request/respond→Rust→授权弹窗 | permissions 默认 manual/60000ms；allowAlways 仅同一会话、完全相同工具与参数；拒绝含 `..` 路径自动放行 |
| `agentd/src/extensions/sandbox.ts`、`K/chat/backend/{host_tools,docker_tools}.rs` | Pi operations→sandbox.request/respond→SandboxExecutor；复用已管理 Docker 容器 | sandbox 超时/输出/shell；probeTimeoutMs=5000、probeCacheMs=10000；v4 返回 exitCode/signal/timedOut；Rust sandboxContainer |
| `agentd/src/extensions/web.ts` | web_fetch、SearchAdapter、BraveSearch、HttpAdapter；已有内存凭据 | web 大小/超时/重定向、search endpoint/credential；SSRF/契约测试，TCP 假服务器受环境限制 |
| `agentd/src/extensions/subagent.ts`、`sessions/registry.ts` | 子会话继承权限/沙箱；全局并发及深度限制；父取消级联 | subagents 默认 2/2/16000；parentCallId 归组；并发/递归/继承/取消测试 |
| `agentd/src/extensions/resources.ts`、`sessions/create.ts` | DefaultResourceLoader；AGENTS 优先、CLAUDE 兼容；专家附加系统提示 | skills.paths 默认 ~/Polaris/skills；experts.enabled=false；上下文/技能/专家开关测试 |
| `src/features/permissions/`、`src/main.ts` | 独立 Vue 确认弹窗、队列，调用 chat_permission_respond | Tauri/HTTP 装配；expiresAt 倒计时、permission.resolved 关闭；队列单测及 vue-tsc，无浏览器点击验收 |

协议已升 v4（两端需一并升级）；R3 入口、协议说明、配置切换、迁移及验证见 [R3 执行报告](pi-migration/r3-result.md)。
P3 初始协议 v3、所有默认值/切换示例、旧容器挂载风险、手工上下文迁移与命令结果见 [P3 执行报告](pi-migration/p3-result.md)。
新增资源配置需重开会话；不自动改名用户文件、不新增依赖、不修改图片/思考档位协议。
基线豁免仍仅 4 项；端口测试与真实安全容器验证未通过前，P3 整体验收为 BLOCKED。


## P3b 请求级专家与验证

| 入口 | 行为、依赖与迁移 | 验证 |
| --- | --- | --- |
| `src/lib/expertSelection.ts`、ChatComposer、`src/stores/{chat,chatTypes}.ts`、`src/components/chat/shared.ts` | 检测 agent_backend_status；Pi 跳过 apply/teamApply，通过 expertSelection={kind,id} 随本轮发送；claude-cli 保留旧写文件行为；基础模式清除选择 | tests/frontend/expertSelection.test.ts 覆盖两种选择、旧后端、状态失败、真实临时目录文件不变与 store 请求字段 |
| `K/chat/types.rs`、`K/chat/backend/{expert_selection,host_tools,bridge}.rs`、`H/wiring.rs` | 可选字段默认 None；ExpertLoader 调只读 expert_export/team_export；失败终止本轮；复用 expertPrompt，未知 kind/类型拒绝；feishu 仅补 None | kernel Pi wire 测试验证本轮 prompt 与 CLAUDE.md/AGENTS.md 内容不变；专家默认 experts.enabled=false 不变 |
| `src/stores/{chatArtifacts,chatDeltas}.ts` | 为满足 500 行上限，从 chat.ts 按职责搬出原有产物处理和 token 缓冲；公开 Bubble/parseArtifacts 导出兼容 | chat.ts 483 行；产物焦点/过滤与 delta 顺序/错误气泡测试；delta 默认 40ms，可显式 createDeltaBuffer(ensureArr, 20) 切换，非有限/负数拒绝 |
| `vite.config.ts` | 保留 Vitest 默认排除规则，追加 agentd/**；node:test 仍用 agentd 自身命令 | 根 Vitest 8 文件/23 测试；vue-tsc；两壳 cargo check；sandbox 13 测试通过 |

专家配置示例：agentd 配置 `{"experts":{"enabled":true,"prompt":""}}` 后重开会话；默认关闭时 expertPrompt 仍传输，但不追加到模型系统提示。此任务未绕过 P3 的显式启用闸。
内核全量 170 通过、10 失败、2 忽略；排除既有基线/端口项后 170 单测 + 4 集成测试通过。
agentd typecheck 通过，npm test 18 文件通过、2 失败（web EPERM；e2e ping 超时）。服务端无 polaris-sandbox 依赖，当前 fail closed；共享策略依赖调整未获批。
当前报告仍为 **BLOCKED**，不代表 Claude / Pi 最终验收；完整命令、失败列表和未运行项见 [P3b 执行报告](pi-migration/p3b-result.md)。

## R4：P3b 审查修复（执行报告，待 Claude / Pi 复核）

仅处理 [P3b2 分流](reviews/P3b2-round1-triage.md) 的 C2、C3、#2、#5、#7、#9，驳回项不改代码。

| 入口 | 行为、公开接口与依赖 | 配置、迁移与验证 |
| --- | --- | --- |
| `S/policy.rs`、`S/policy_tests.rs`、`S/lifecycle.rs` | resolve_mounts/run_args/validate_container 保留 canonical 路径与双向保护前缀，显式禁止 HOME 及祖先；删除名称匹配和目录遍历；path_for_docker 注释说明 Windows 前缀/盘符 | 沿用 POLARIS_SANDBOX_CONFIG，protectedPaths 默认 []、仅追加且须绝对路径；无需迁移配置。普通 src/data、sessions、fixtures/x.key 可挂载；17 项测试含假 Docker 创建到路由、保护上下层及 0/2048 文件的禁止枚举验证 |
| `K/chat/backend/expert_selection.rs` | load 的缺加载器、加载失败、空 id 错误含 kind/id；加载器原始错误可能带提示词，故仅暴露选择元数据 | ExpertLoader 接口不变，无新依赖；默认无选择；expert/team 两类失败与提示词不泄露测试 |
| `K/chat/backend/pi/extension_tests.rs`、`src-tauri/crates/polaris-kernel/tests/fixtures/fake-agentd.mjs` | 同 session 的两轮使用不同 request id；done 与命令响应同步；fixture 在派发前 fsync 记录；删除固定间隔预约轮询 | 无生产配置变化；连续 10 次未过滤 kernel lib 全量运行，目标测试 10/10 通过，每次仍有 10 项许可例外失败 |
| `src/features/sandbox/{project.ts,project.test.ts,components/SandboxProject.vue}` | projectDirectoryHint 经 computed/aria-live 即时显示 POSIX、Windows 盘符绝对路径格式提示；最终仍调用后端校验 | 默认空目录提示选择；/home/me/project、C:/Projects/app 或反斜杠盘符路径；无新配置；vue-tsc、Vitest 25 项通过 |

两壳 offline check 通过；内核排除既有 4 项基线与 6 项 TCP 限制后，171 单测和 4 集成测试通过，2 忽略。
默认值、配置切换例、全部失败名称、命令/日志、未运行项见 [R4 执行报告](pi-migration/r4-review-fixes.md)。
历史 P3b 报告的名称黑名单/全树扫描说明已由本节取代；服务端策略依赖和真实 Docker 验收边界仍按历史记录保留。

## P4 提供商、凭据与用量（执行报告，验收 BLOCKED）

| 入口 | 依赖与行为 | 配置、迁移与验证 |
| --- | --- | --- |
| `K/provider/{pi_translate,pi_config}.rs`、store 门面 | ProviderView→Pi providers/defaultModel；显式 models→env→同提供商内置目录；具名协议映射 | pi-default.json；POLARIS_PROVIDER_CONFIG；类型校验、模型/价格/alias 覆盖；三协议与优先级测试 |
| `K/provider/credential_file.rs`、`agentd/src/models/file-credentials.ts` | 方案 A：共享 providers.json.credentials，mkdir 锁覆盖刷新/保存，原子替换；API key 从 env 同文件迁移 | 默认锁等待 30000ms、轮询 25ms；旧外部 OAuth 不复制，Pi 首次重登；假时钟/端点并发刷新和重开持久化测试 |
| `K/chat/backend/pi/`、`agentd/src/sessions/registry.ts` | provider_switch→config.apply；空闲自动重建保留历史，运行会话本轮后应用；Pi 不启动 codex_proxy、不写 Claude 全局配置 | 协议 v5；SDK SessionManager / ModelRuntime；切换、历史保留、新会话、版本指纹验证 |
| `K/provider/pi_login.rs`、`agentd/src/models/logins.ts` | provider.auth 复用 anthropic/openai-codex 原生登录/刷新；Rust 登录接口保留旧后端分支 | oauthTimeoutMs=600000；Codex 默认 device code；登录交互不返回 token |
| `agentd/src/events/usage-store.ts`、`K/provider/{usage,pi_usage}.rs` | 每条权威 assistant usage 按会话/提供商/模型持久化；与 Claude JSONL 按日期及明确请求身份合并 | Pi 默认由 Rust 指向 Polaris/data/pi-usage；独立 agentd usageDir=""；记录去重测试 |
| `K/provider/store/failover.rs`、Pi Translator | MODEL_AUTH/RATE_LIMIT/NETWORK/ERROR 接既有滑窗；成功清零，取消/上下文/工具错误不切换 | 无新增滑窗依赖；错误分类及阈值测试，真实网络故障切换未验收 |
| `agentd/src/extensions/permissions.ts` | resolved 通知发送/日志失败不改变本地决策 | 固定日志文案；allow/cancel/expiry 发送失败测试 |

全部文件/行数、SDK d.ts 证据、默认值与切换示例、凭据与账本迁移、命令及失败名单见 [P4 报告](pi-migration/p4-result.md)。
两壳 check、vue-tsc 和过滤后的 kernel 测试通过；agentd 全量仍有 web EPERM 与 e2e ping 超时，不能宣称完成。
前端开关/登录/用量文案需配合；没有直接修改前端、没有新增依赖、没有 commit/push。
