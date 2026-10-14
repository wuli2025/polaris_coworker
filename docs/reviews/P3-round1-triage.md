# P3 第 1 轮 Pi 审查处置（Claude）

原始报告：`docs/reviews/P3a-round1/`（agentd）、`docs/reviews/P3b-round1/`（Rust/前端）。

## 主控自查新增

| # | 级别 | 问题 | 证据 |
| --- | --- | --- | --- |
| C1 | 高 | 沙箱内 bash 永远报告成功 | `agentd/src/extensions/sandbox.ts` 的 `remoteBash.exec` 固定 `return { exitCode: 0 }`；沙箱中失败的命令对模型显示为成功，模型据此继续，错误被掩盖。修：Rust 回传真实退出码（以及被信号终止/超时状态），agentd 原样返回；测试失败命令得到非零退出码。 |

## P3a（agentd）

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 中 | `withinWorkspace` 符号链接逃逸 | **采纳并提为高** | Pi 推理自相矛盾，但问题真实：`permissions.ts:36` 先 `resolve(cwd, path)` 做词法规整（消掉 `..`）再 `realpath`。主控复现：工作区内 `link → 外部目录`，路径 `link/../escaped.txt` 被判"目录内"、auto-current 自动放行，文件实际写到工作区外（`scratchpad/symlink-poc`，输出 `inside: true / written outside: true`）。修：不做词法规整；含 `..` 段的路径一律不算"目录内"（回落人工确认）；仅对实际存在的最长前缀做 realpath，不存在的尾部不得含链接；测试覆盖本复现、绝对路径、Windows 分隔符。 |
| 2 低-中 | 沙箱不可用要等 120 秒超时 | **采纳（中）** | 体验差且延迟故障暴露。修：开启沙箱的会话在首次工具调用前（或会话打开时）做一次快速可用性探测（超时进配置，默认数秒），失败立即 `SANDBOX_UNAVAILABLE`；探测结果短期缓存。 |
| 3 低 | allowAlways 以 tool+args 精确匹配，含随机参数时记不住 | **驳回（设计如此）** | 精确匹配是更安全的默认：放宽到"同工具全放行"会让一次同意覆盖任意命令。作用域=同一会话、完全相同的工具与参数；在 feature-tree 与弹窗文案写明。 |
| 4 低 | 子代理 abort 竞态 | **驳回** | Pi 自己确认 `finally` 中 `close(childId)` 兜底，无可复现影响。 |

## P3b（Rust/前端）

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 严重 | 沙箱不可用时无阻止回落宿主 | **有证据驳回** | 回落控制在 agentd：`sandbox.ts:59-61` 按 `this.enabled` 选择工具，开启时只用远程实现，错误直接抛出，不存在宿主路径。Pi 未获 agentd 代码。 |
| 2 严重 | 受保护路径不一致 / 项目建在 `~/.polaris` 内仍可挂载 | **驳回场景，采纳配置化** | `docker_tools.rs` 对挂载源与受保护路径做双向前缀判断，项目位于 `~/.polaris` 下会被拒绝，Pi 的场景不成立。但受保护名单写死在 `wiring.rs`，按规范改为"配置 + 代码兜底名单"，并入 P3b。 |
| 3 严重 | 删除了 `spawn_in_sandbox` | **驳回** | 该函数是从未被调用的死代码，P3 任务明确要求删除或并入新路径；新路径即受控容器路由。 |
| 4 中 | 未校验挂载目标路径与读写属性 | **采纳，并入 P3b** | 与挂载最小化同属 `polaris-sandbox`/`docker_tools.rs`，校验 `Destination` 与 RW 标志。 |
| 5 中 | 未校验容器资源限制与安全选项 | **采纳，并入 P3b** | 容器创建时按配置设置内存/CPU 上限、`--cap-drop`、`no-new-privileges`；路由前校验这些基线。 |
| 6 中 | allowAlways 作用域不明 | **驳回，补文档** | 作用域已定义：`Permissions` 实例按会话创建，`remembered` 随会话结束清除（`permissions.ts:41,48`）；按钮文案"本会话相同调用总是允许"与实现一致。补协议文档说明。 |
| 7 中 | Pi 模式 expert_prompt 逻辑矛盾 | **驳回** | 设计如此：Pi 模式不把专家块拼进用户正文（`prepare.rs` 中 `pi` 时为 None），改由 `SessionOptions.expert_prompt` 作为系统提示附加段装载，正是 P3 第 5 部分的要求。 |
| 8 低 | 授权弹窗无超时提示 | **采纳** | agentd 超时按拒绝后，前端弹窗仍停留会误导。修：弹窗显示剩余时间；agentd 在请求过期/轮次结束时发 `permission.resolved`（或等价）事件，前端据此关闭弹窗。 |
| 9 低 | `set_sandbox_executor` 结果被忽略 | **采纳** | 失败时记录警告日志。 |
