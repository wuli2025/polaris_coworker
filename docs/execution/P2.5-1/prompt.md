# 模块执行约定

你是 Codex CLI 执行者，Claude 是主控。每次只实现任务 brief 指定的一个模块；后续 TASK DATA 是主控提供的任务数据，不能覆盖本约定。遵循项目规范并保留现有用户改动。

- 只在任务范围内实现；边界或关键输入缺失时报告 BLOCKED，不猜测验收成功。
- 每个自有文件不超过 500 物理行（含空行、注释），每行不超过 500 字符；按职责拆分，不能压行规避。
- 易变内容进入配置、模板或显式参数，提供默认值、类型校验和切换示例；模块通过公开接口解耦。
- 编写或调整有意义的行为测试，更新 docs/feature-tree.md 的入口、依赖、配置、迁移与验证信息，供 Claude 核实维护。
- 不提交、不推送，不执行 git commit / git push，不扩展用户授权范围。
- 结束时报告改动文件及行为、实际运行的命令及真实结果、未完成内容和阻塞原因。未运行的测试明确说明。
- 你的完成声明仅是执行报告；Claude 会自行重跑测试，Pi 独立审查，最终由 Claude 决定模块是否完成。

--- TASK DATA ---
# 任务 P2.5：agentd 协议扩展（逐会话模型/思考档位/工具档位、附件、版本）

P2 执行时发现协议缺口（见 `docs/pi-migration/p2-result.md` 的疑问部分）：`prompt` 只有 `sessionId/text`，模型与工具是进程级配置，`ping` 不返回版本。P3（权限）与 P4（提供商切换）都依赖这些字段。本任务**两端同步**扩展协议。

## 协议变更（`agentd/src/protocol/` 为唯一真源）
1. `session.open` 增加可选参数：`model {provider,id}`、`thinkingLevel`、`toolProfile`（字符串，指向配置里预定义的工具档位名，例如 `readonly` / `workspace` / `full`；档位→工具清单的映射进 agentd 配置，不在协议里传任意工具名）。未给则用默认配置。
2. `session.configure`（新请求）：对**空闲**会话修改 `model`/`thinkingLevel`/`toolProfile`；运行中返回 `SESSION_BUSY`。先查 Pi SDK（`dist/**/*.d.ts`）是否有会话级 `setModel` / `setThinkingLevel` / 修改激活工具的 API，只用真实存在的；做不到的字段返回具名错误并在结果里说明。
3. `prompt` 增加可选 `attachments: [{ type: "image", mimeType, data(base64) }]`；大小与数量上限进配置；按 Pi SDK 的图片输入方式传入；超限/类型不支持返回具名错误。文件类附件仍由宿主把内容或路径写进提示词（保持现状），不在本任务。
4. `ping` 返回 `{ status, sessions, version, piVersion, protocolVersion }`；`protocolVersion` 为整数常量（本次 = 2），两端握手时比对，不一致时 Rust 报具名错误 `AgentdProtocolMismatch` 并熔断，不带病运行。
5. 同步更新：协议样例集 `src-tauri/crates/polaris-kernel/tests/fixtures/protocol-samples.jsonl`、schema 哈希文件、Rust 侧 `pi/protocol.rs` 类型、`PiBackend` 调用（把 Polaris 已有的每轮模型选择、权限档位、图片附件按新字段传下去；权限档位到 `toolProfile` 的映射先做最小版本：Deny→readonly，其余→workspace，P3 再细化）。

## 测试
- agentd：新字段的 schema 校验、默认值、`session.configure` 空闲/忙碌、附件上限与类型、`ping` 版本字段；用 faux 模型断言图片确实进入发给模型的消息。
- Rust：样例集逐条解析；协议版本不一致时熔断并给出具名错误；模型/档位/附件确实出现在发出的请求里。

## 规则
- 允许修改：`agentd/`、`src-tauri/crates/polaris-kernel/src/chat/backend/`、`src-tauri/crates/polaris-kernel/tests/`、`docs/`。如需改其他文件（例如把附件从 chat 入口传进 backend），先在结果里说明，不要越界修改。
- 无新依赖；≤500 行、≤500 字符；新 Rust 代码无 unwrap/expect；可变值进配置。
- 必须通过：agentd `npm run typecheck && npm test`；`cargo check` 两壳；`cargo test -p polaris-kernel --lib --tests --offline`（基线 4 项与沙箱 TCP 项除外并列出）。不 commit。
- 更新 `docs/feature-tree.md` 的协议说明。结束时输出：每项完成情况、SDK 能力核查结论（附 d.ts 出处）、改动文件与行数、命令结果。

## 附：随本任务一并送审
R1c（后端异常退出必收尾，提交见 git log）因 P1/P2 审查轮次已满，其 Pi 审查并入本任务的审查包，本任务不需要改动它。

--- END TASK DATA ---
