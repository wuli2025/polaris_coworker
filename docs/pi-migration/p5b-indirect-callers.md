# P5b 间接调用方迁移执行报告

本报告是 Codex 执行记录，不替代 Claude 重跑或 Pi 独立审查。不提交、不推送。

## 行为差异与入口

| 项目 | 原行为 / claude-cli 兼容分支 | Pi 行为 |
| --- | --- | --- |
| 调用 | 独立 `claude --print`，stream-json | `AgentBackend::start_with_tools`，没有 Claude fallback |
| cwd | 知识库根目录 | 知识库根目录下的 `wiki/`，规范化且拒绝指向库外的目录链接 |
| 写权限 | 原 CLI allowedTools 与提示词约束 | 固定 `auto-current`；agentd 执行前检查 cwd 范围；无 UI 权限请求立即 deny 并 abort |
| 工具 | 原 Read/Write/Edit/Glob/Grep | `toolProfile` 配置，默认 workspace；档位存在性由 agentd 验证 |
| 沙箱 | 不支持；选择沙箱时报错 | 传递用户 `sandbox`，调用现有 SandboxExecutor；没有 executor 时失败，不降级宿主 |
| 进度 | 看到 Write/Edit 调用就计页面 | tool.started→tool；tool.completed 成功且路径在 wiki 内→page；text.delta→delta |
| 计数 | 写入调用路径去重 | 成功完成的规范化页面路径去重，失败工具不计页面；可恢复工具错误仍可最终成功 |
| 收尾 | 保留原 error/done/重扫语义 | 启动失败、运行错误、超时、取消均重扫并恰好一次 done；abort 后有界等待，再 close |
| 超时 | 原 watchdog 杀进程树 | watchdog 和流接收超时均调用后端 cancel→abort；兼容分支也通过后端 cancel 回收原进程池 |

公开入口：`wiki::kb_compile(AppHandle)` 不变；新增 `compile_with_config(AppHandle, CompileConfig)`
供宿主显式传入用户选择，`cancel_compile(run_id)` 走后端 cancel。
前端未改，仍订阅 `kb:compile`，字段保持 runId/kind/text/docCount。
当前 wiki IPC 没有沙箱参数；使用下面的 wiki 配置保存选择，宿主 API 可显式覆盖。
新入口与旧入口均先获得 fable 维护锁，直到后台线程收尾释放。
旧路径只在 selected backend 为 `claude-cli` 时可达；Pi 出错不会启动 Claude。

依赖仍为 wiki→kernel/fable/runtime，无新依赖。
kernel 的公开扩展仅为 `TurnRequest::headless`、`ToolOptions.tool_profile/detailed_events`。
详细事件 opt-in；普通 chat 不新增事件或改变 done 字段，前端无需适配。

## 配置与切换

`POLARIS_WIKI_CONFIG` 指向 JSON 文件；不设置时使用以下默认值：

```json
{"toolProfile":"workspace","sandbox":false,"timeoutSecs":3600,"abortGraceMs":5000}
```

- 字段类型严格检查，未知字段拒绝；toolProfile 不允许空白；timeoutSecs 为 1..604800 的整数。
- abortGraceMs 为 1..60000 的整数；等待 abort terminal 的上限，不替代 agentd 自身 abortTimeoutMs。
- 兼容 `POLARIS_KB_COMPILE_CAP_SECS`，优先于 JSON；非法值现在明确报错，不静默回退默认值。
- 示例：`{"toolProfile":"workspace","sandbox":true,"timeoutSecs":1800}` 保留沙箱选择。
- 自定义档位示例：在 agentd 配置的 `toolProfiles` 中定义
  `"wiki-compile":["read","grep","find","ls","write","edit"]`，wiki 配置改为 `"toolProfile":"wiki-compile"`。
- `workspace` 可能包含 bash；wiki 对所有需要确认的操作直接 deny，不自动授权执行。
- Pi 提示模板在 `W/wiki/compile/prompt-pi.md`；raw 与规则按知识库绝对路径读取，写入使用 wiki 内相对路径。
- 沙箱沿用现有容器策略和安装的 executor；本任务没有改变容器挂载、安全基线或服务端隔离模式。

后端切换继续使用现有设置：`POLARIS_AGENT_BACKEND=pi`（默认）或 `claude-cli`，
或 `POLARIS_AGENT_CONFIG` 中 `{"agent":{"backend":"claude-cli"}}`。
`selected()` 为进程级选择，切换后重启宿主。默认 server 的 per-account 若没有可信身份仍拒绝，不绕过 P2 隔离策略。

远程执行名单由 `H/exec/defaults.json` 提供全部既有默认项，包括 claude/codex，避免破坏旧用户。
`POLARIS_EXEC_CONFIG` 指向替代 JSON，例如 Pi 主机仅允许：

```json
{"allowedPrograms":["node","git","cargo"]}
```

Pi 后端本身不需要 claude，无需为它开放该远程命令。
allowedPrograms 必须为字符串数组；空数组关闭所有白名单命令；未知字段、路径、扩展名、非法名字拒绝。
展示与执行都读取相同配置；配置错误时展示空名单，执行返回错误，不回退宽名单。
保留原总开关、鉴权入口、Shell 临时授权、命令路径/元字符校验及审计。

命名迁移：`run_agent_readonly` / `run_agent_readonly_timeout` 替代旧名，fable 内调用全部更新。
旧名在 kernel 保留 `#[deprecated]` 转发一个迁移版本，fable 的 kb 门面保留兼容 re-export。
参数、只读权限、回调、超时、旧后端文本修剪行为不变。

## 验证与限制

最终命令结果、文件与物理行数见 [执行清单](p5b-files-and-validation.md)。
测试使用假的 AgentBackend 验证启动参数、进度、拒绝确认、取消/超时及失败收尾；
kernel 另用 P2 假 agentd 验证 session.open 上的 cwd/profile/auto-current/sandbox 和详细事件适配。
没有启动真实 Claude、付费模型或真实 Docker；未做 Windows/macOS 真机运行和前端点击验收。

目录安全测试覆盖相对穿越、绝对库外路径、已有/新文件符号链接逃逸、悬空链接。
执行前安全边界复用 agentd 已有 Permissions；Rust 的路径验证负责拒绝不合法进度，不能替代执行前检查。
本任务未修改 agentd 权限实现，也不把假后端测试表述为真实模型/容器端到端证明。

## 并行改动与技术债

开始时已有未跟踪 `docs/execution/P5b-1/`。执行期间观察到 doctor 文件及 R6 文档被其他执行者修改，均未覆盖。
首次 wiki check 遇到并行 doctor `ResolvedShell/as_deref` 编译错误；后续已消失。

fable 只做名称/注释修改及两个获准的跨平台测试修复；既有 >500 行文件未扩展业务职责，
保留在文件清单中标为技术债，未为改名扩大重构范围。exec 原 578 行已拆出配置与测试；wiki 按配置、模板、生命周期、进度、兼容分支拆分。
