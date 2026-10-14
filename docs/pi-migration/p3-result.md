# P3 执行报告（待 Claude / Pi 复核）

分支：`refactor/pi-agent`。未提交、未推送、未添加依赖；保留原有 `docs/execution/P3-1/`。
本报告不是最终验收声明。完整验收受到当前执行环境阻塞，见下文。

## 五部分实现

| 部分 | 入口与行为 | 验证与边界 |
| --- | --- | --- |
| 四档权限 | `agentd/src/extensions/permissions.ts`；强制装载 `tool_call` 闸；Rust `pi/input.rs` 同时发送工具档及权限档 | 16 格矩阵、允许/拒绝/超时/allowAlways、symlink、真实 SDK 审批/取消；Rust 确认往返测试 |
| 真沙箱 | `extensions/sandbox.ts` 替换 Pi bash/write/edit operations；协议→Rust `host_tools.rs`→`docker_tools.rs` | 假后端验证无宿主执行、不可用报错；实际容器执行未验证；旧宽挂载必须手工迁移 |
| 联网工具 | `extensions/web.ts`；web_fetch、BraveSearch/HttpAdapter；凭据来自已有内存 CredentialStore | SSRF、大小/超时/重定向、搜索适配器契约；本地 HTTP 假服务器受 EPERM 阻塞 |
| 子代理 | `extensions/subagent.ts`、`sessions/registry.ts`；注册表子会话、全局子任务并发、深度上限、级联取消 | 权限/沙箱继承、父调用分组、递归/并发限制、父取消；子终态不提前结束父轮次 |
| 上下文/技能/专家 | `extensions/resources.ts`、DefaultResourceLoader、before_agent_start | 同目录 AGENTS.md 优先，否则 CLAUDE.md；配置技能扫描；专家默认关，开启时附加系统提示 |

权限与沙箱属于强制宿主扩展，不受 `extensions.enabled=false` 关闭；该开关只控制可选可信扩展。
未知工具默认按需要批准处理；readonly 工具名单为 read/grep/find/ls/web_fetch/web_search/subagent。
subagent 仅负责调度，子会话仍经过相同权限闸。工具可见性继续由 toolProfiles 独立决定。
`allowAlways` 只记住当前会话中**相同工具及完整参数**，不跨会话/重启/重新配置权限档继承。
工作区写权限检查会解析符号链接及最接近的现存父目录；这不是对并发恶意替换文件系统的 OS 隔离承诺。

## 前端及 Rust 接口

- 新增 `src/features/permissions/PermissionDialog.vue`、`queue.ts`，在 `src/main.ts` 独立挂载。
- `chat:stream` 新增 `kind=permission.request`，`text` 为 JSON：sessionId、callId、tool、summary、parentCallId。
- 弹窗提供拒绝/允许一次/本会话相同调用总是允许；完成事件清理该请求的待审批队列。
- `chat_permission_respond({response:{sessionId,callId,decision}})` 已注册桌面 Tauri 和 HTTP 壳。
- HTTP 延用现有 owner 角色授权，不扩大其他角色权限。无人交互时按 agentd 超时拒绝。
- Rust ChatStreamEvent 增加可选 callId/parentCallId，用于子工具归组；旧构造点显式填 None。
- 非 UI 调用者使用公开 AgentBackend.start_with_tools(TurnRequest, ToolOptions) 传递沙箱与专家内容，不借用旧后端上下文。
- 当前普通工具列表仍沿用原展示；分组可使用上述新增字段，未重写聊天组件。
- UI 队列行为测试和 vue-tsc 已运行；真实浏览器/桌面点击演练未运行。

## 配置、默认值与切换

agentd 配置文件仍经现有启动参数传入，或 `config.apply` 更新。
嵌套对象是整对象替换；下面给出完整示例片段。更改资源/扩展配置后按 reopenRequired 重开会话。

```json
{
  "permissions": {"defaultMode":"manual", "timeoutMs":60000, "summaryChars":2000},
  "sandbox": {"timeoutMs":120000, "maxOutputBytes":262144, "shell":"sh"},
  "subagents": {"maxConcurrent":2, "maxDepth":2, "summaryChars":16000},
  "skills": {"paths":["~/Polaris/skills"]},
  "experts": {"enabled":false, "prompt":""},
  "web": {
    "timeoutMs":15000, "maxBytes":1048576, "maxRedirects":3,
    "search": {
      "endpoint":"https://api.search.brave.com/res/v1/web/search",
      "credential":"web-search", "maxResults":5
    }
  }
}
```

TypeBox 拒绝错误类型、未知配置键、非法枚举和负限额；超时/字节/并发必须为正整数，深度/重定向允许 0。
只读/询问/工作目录自动/全部自动分别为 `deny/manual/auto-current/auto-all`。
通过 `session.configure` 同时切换 `{toolProfile:"workspace", permissionMode:"auto-current", sandbox:true}`；运行中拒绝重新配置。
readonly 仍可联网；workspace 新增 bash，但执行是否允许由权限闸决定。

联网默认工具可见；搜索 Key 不存在时明确 `WEB_SEARCH_CREDENTIAL_REQUIRED`，不会伪装成成功搜索。
现有 apihub/provider store 未发现独立搜索供应商配置，所以新增 Brave HTTP 适配器，不复用模型 Key。
用 `config.apply` 的 `credentials:{"web-search":"<由宿主注入的Key>"}` 注入，勿写入配置文件、日志或提示词。
可以替换 SearchAdapter/HttpAdapter 实现；endpoint 可对接返回同样 Brave JSON 契约的代理。
web_fetch 只允许 HTTP(S)，检查全部 DNS 地址并固定连接地址，每次重定向重查；跨源重定向不转发凭据。
没有生产私网豁免开关；本地假服务器只由测试显式替换 HTTP 适配器。

开启专家：`experts:{enabled:true,prompt:"额外专家准则"}`；Rust 现有 ExpertBridge 路由/召集内容通过 expertPrompt 装载。
`expert_apply`/团队应用等手动写文件入口保留；Pi 对话拼装不再调用旧 CLAUDE.md 渲染或把专家块放进用户正文。
**仍待处理的前端边界**：ChatComposer 的显式专家/团队选择仍调用 expert.apply/teamApply 写文件。
本任务仅获准对前端授权交互做最小改动，未改这些专家选择入口；单专家模式仍依赖其既有上下文文件。
因此“所有专家选择入口均彻底脱离写文件”尚未完成，需主控另定该前端迁移范围。
旧文件内已存在的专家内容仍属于用户上下文，关闭专家开关不会删除它。

## 协议迁移

协议升到 **3**：permission.request/respond、sandbox.request/respond，以及 session 的 permissionMode/sandbox/expertPrompt。
请求的 targetSessionId 保留子会话身份；子事件外层 sessionId/turnId 归到父轮次，parentCallId 指向发起调用。
Rust 握手、JSONL 样例、协议源 SHA256 与假 daemon 全部同步；图片和思考档位字段未改。
两端需一起升级，否则命名错误 AgentdProtocolMismatch 立即熔断。

## 容器与上下文迁移（doctor 核实项）

复用 polaris-sandbox 现有容器的管理方式和镜像；Rust 工具路由通过 docker inspect/exec 使用受控容器，未新增 crate 依赖。
桌面/服务端 `wiring.rs` 注入 SandboxExecutor；Rust 配置 `agentd.sandboxContainer` 默认 `polaris-sandbox`，可用 `{"agentd":{"sandboxContainer":"polaris-project-tools"}}` 指定另一个既有容器。
容器名限定 Docker 名称字符；检查实际 bind mounts、运行状态、privileged、host PID/network。
容器只接收工具操作及内容，没有传入 agentd 环境变量或模型 Key。

**现有 sandbox_start 会把 ~/Polaris 整体挂到 /workspace，其中 data/providers.json 包含明文 Key。**
因此本次路由会拒绝这种容器，报 SANDBOX_UNSAFE_MOUNT；不可静默继续使用它。
需要运维手工准备只挂载目标项目目录的容器，或后续获准修改 polaris-sandbox 的创建挂载策略。
本任务范围未包含该 crate，未修改它、未重建/删除用户容器。
保护路径由装配点显式传入（Polaris/data、.polaris、.pi、.claude、.codex、.ssh、.aws）；自定义凭据目录需要装配点补充。
目标 cwd 必须位于容器已存在的可写挂载下；其他宿主路径报 SANDBOX_PATH_UNMOUNTED。命名卷不能核实来源，会被拒绝。
旧镜像只有 sh，默认 shell=sh；需要 bash 语法时配置 shell=bash，并由运维确保镜像中有 bash。
容器内 timeout 限制命令时长；取消会停止子会话/RPC 等待，已经启动的 Docker 命令可能运行至该上限。
legacy 后端 useSandbox 明确报错，原 spawn_in_sandbox 死代码已删除。

项目只有 CLAUDE.md 时兼容读取；同目录同时有 AGENTS.md 时只使用后者。
doctor/运维核查后可手工复制或改名 CLAUDE.md→AGENTS.md，先比对合并内容；程序不自动改名、不覆盖用户文件。
Polaris 默认技能目录 `~/Polaris/skills` 通过 DefaultResourceLoader.additionalSkillPaths 装载。

## 实际验证

实际命令及结果如下；日志位于 `/tmp/polaris-p3-validation/`，不修改 baseline.md 的豁免名单。
Rust 命令在 src-tauri/ 执行，agentd 命令在 agentd/ 执行，其余在仓库根。

| 命令 | 最终实际结果 |
| --- | --- |
| `npm run typecheck` | 通过，退出 0 |
| `npm test` | 17 个测试文件通过、2 个失败，退出 1；e2e 子进程 ping 前退出 0，HTTP 假服务器 listen EPERM |
| `cargo check -p polaris-app --offline` | 通过，退出 0，6 个既有 kernel 警告 |
| `cargo check -p polaris-cli --bin polaris-server --features collab-net --offline` | 通过，退出 0 |
| `cargo test -p polaris-kernel --lib --tests --offline` | 165 通过、10 失败、2 忽略，退出 101；4 个基线失败加 6 个端口相关失败 |
| `cargo test -p polaris-kernel --lib chat::backend --offline` | 65 通过，退出 0 |
| `cargo test -p polaris-kernel --test backend_config --offline` | 4 通过，退出 0；全量命令在 lib 失败处停止，因此单独补跑 |
| `npx vue-tsc --noEmit` | 通过，退出 0 |
| `npx vitest run src/features/permissions/queue.test.ts` | 2 通过，退出 0 |
| `node --import tsx --test --test-name-pattern='socket-free\|SSRF' test/web.test.ts` | 不依赖本地监听的 SSRF/适配器/限额测试通过，退出 0 |
| 指定 `check_lines.py . --files <本次文件清单> --extensions .md,.jsonl,.sha256 --limit 500` | 通过；另外逐文件扫描单行长度，均 ≤500 字符 |
| `git diff --check` | 通过 |

曾运行的诊断：直接执行 `test/web.test.ts` 定位 listen EPERM；`test/e2e.test.ts` 定位子进程提前退出；TCP/Unix socket 探针均 EPERM。
早期复跑曾发现并修正协议版本夹具、测试显式权限档、测试参数类型和 SDK 路径标准化差异，以上表格采用修正后的结果。
行数脚本首次传多个位置参数被拒绝，按其 --files 接口重跑通过。

- 已观察到新增权限/沙箱/资源/子代理测试通过；生产默认 manual 单独断言，既有 SDK 工具测试显式选择 auto-all。
- 当前环境 TCP/Unix socket 监听均 EPERM，搜索假服务器无法运行。
- 实际 Docker 运行、外部搜索 Key、真实模型冒烟未运行；没有凭据注入，也没有启动容器。
- 指定的内核全量测试除 4 个基线失败外还有 6 个端口相关失败，不能按用户验收标准宣称通过。
- P3 整体验收为 **BLOCKED**：需要允许本地监听的测试环境、不暴露凭据的容器，以及显式专家选择入口的后续前端迁移。

逐文件清单和物理行数见 [p3-files.md](p3-files.md)。
