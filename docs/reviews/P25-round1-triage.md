# R1c + P2.5 + P2.5b 第 1 轮 Pi 审查处置（Claude）

原始报告：`docs/reviews/P25-round1/`。

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 严重 | `TurnStream::process()` 持 Mailbox 锁执行轮次，panic 时死锁，error→done 发不出 | **有证据驳回** | `claude_cli/pipeline/stream.rs` 中没有任何 `lock()` 调用，不持有 Mailbox 锁；Mailbox 锁只在 `stream.rs` 的 `finish`/`push` 内短暂持有。且 parking_lot 的锁守卫在 panic 展开时随栈帧释放、不中毒。Pi 描述的 `StreamGuard::process` 不存在。 |
| 2 严重 | 图片读取 TOCTOU（metadata 无 O_NOFOLLOW） | **驳回，记录为已知限制** | `file.metadata()` 是对已用 O_NOFOLLOW 打开的 fd 做 fstat，Pi 自己也承认这部分正确。剩余风险只有"规范化后、打开前，上级目录被替换为符号链接"，前提是攻击者对会话 uploads 目录有本地写权限；远程协作者只能提交路径字符串、无法写本机文件系统，不在威胁模型内。在 `docs/feature-tree.md` 的附件节点记录此限制；若将来 uploads 目录对他人可写，再改为 Linux `openat2(RESOLVE_BENEATH)`。 |
| 3 中 | supervisor 退出时 `deferred` 未处理，请求丢失 | **部分采纳（低）** | 不会挂起：`State` 被丢弃时 `deferred`/`pending` 中的回复通道随之丢弃，调用方立即收到错误。但该错误在 `client.rs:53` 被统一映射成 `AgentdRequestTimeout`，把"supervisor 退出"误报成"超时"。修：`State::drop` 显式用 `AgentdSupervisorExited` 回复 `deferred` 与 `pending`；`client.rs` 区分"通道关闭"与"真正超时"。 |
| 4 低 | configure 失败后 toolProfile 过期 | **改述后采纳（低）** | `registry.ts:85` 只在成功后更新 `toolProfile`，Pi 的说法不成立。但真实问题是**部分生效**：`configureSession` 依次改模型、思考档位、工具，中途失败时前面的修改已生效，会话处于混合状态。修：先全部校验（模型存在、档位合法、档位名存在、模型支持所需输入）再一次性应用；应用阶段失败则回滚到原值；测试中途失败后会话设置完全不变。 |

## 第 2 轮（复审 `P25-round2/`）
- Pi 确认 #3、#4、#2 记录均正确修复。唯一意见（低）：`options.ts` 中 `thinkingAttempted` 在 `setModel` 后也置真，语义混合。**驳回（非阻塞）**：代码已有注释说明原因（"setModel also changes thinking level, including before a possible failure"），Pi 亦确认回滚行为正确。
- Claude 轻量自检：`SUPERVISOR_EXITED` 唯一定义于 `pi/routes.rs:5`，所有引用经常量；`configure-transaction.test.ts` 覆盖预检与回滚。
- 结论：R1c、P2.5、P2.5b、R2 **标记完成**（已达 2 轮上限，无剩余阻塞）。
