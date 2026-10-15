# F1：消除测试时序依赖

执行范围是 F1；未 commit/push、未新增依赖。工作区原有 R8/P6a 等改动保留。
这份报告不是验收声明；验证状态见 [实际命令与证据](f1-validation.md)。

## 根因与证据边界

1. **agentd E2E 首次 ping**：原 `helpers.until()` 给所有操作固定 5000ms，
   从父进程写入 ping 开始计时，包含子进程启动、tsx/SDK 加载和 daemon 初始化。
   外层 15000ms 还要容纳 EOF、SIGTERM 两次完整启动。
   全量测试启动多个 SDK worker 时竞争 CPU/I/O，5 秒不是就绪信号。
   源码已有 ping 响应，故不需要增加生产 ready 协议。
   本沙箱另外存在子进程管道异常：最小 Node 探针即出现 EPERM/空 stdout，
   新等待器会立即报告 daemon 提前关闭；不能据此宣称外部首次 ping 偶发问题已经实测消失。
2. **`idle_reaping_and_reopen_after_config_apply`**：基线全量实际复现
   `SESSION_NOT_FOUND`，发生在第二次 `collect` 的 prompt。
   100ms idle 从 actor 最后收到控制消息算起；测试线程/Node 被调度延迟后，
   `session.open` 与 `prompt` 之间也可能跨过回收条件。延长断言 deadline 无法修复这个竞态。
3. **`idle_daemon_reopen_restores_experts_before_releasing_requests`**：
   150ms idle 从 supervisor 初始化起算，不保证专家恢复完成后测试线程还有观察窗口。
   同一 actor 循环可以发布 `experts_synced=true` 后立刻回收并发布 false，
   5 秒轮询就永远等不到曾经短暂出现的 true。本次基线未复现该项，根因来自状态路径分析。
4. **共享状态与相邻风险**：原 Pi 公共测试 config 继承真实用户的专家设置路径；
   session map 用 PID+进程内计数，跨进程 PID 重用会碰到遗留目录。
   agentd harness 仅隔离 sessionDir，usageDir 仍取共享默认值；E2E 还继承宿主 provider store 环境。
   这些都是测试隔离缺口，但未把它们说成已实测触发上述三个失败的唯一原因。

## 同步接口、配置和迁移

- `agentd/test/synchronization.ts` 提供 `Changes.wait()`、`notify()`、`close()` 和 `bounded()`。
  谓词在订阅后立即检查，支持事件早于订阅、晚于订阅以及关闭后已缓冲的终态。
  等待结束即移除监听器；提前关闭直接失败；deadline 仅作为兜底。
- E2E 在 JSONL reader 解析回调通知等待器；仍先发送 ping，再等真实响应。
  退出改等 `close`，确保 stdout/stderr 已排空后验证终态和日志。
  EOF/SIGTERM、协议 schema、错误码、工具输出、脱敏断言全部保留。
- Rust `test_support` 通过 `Client::send` 的 **cfg(test)** 控制消息推进单个 actor 的 idle 年龄，
  前置请求完成后再触发原有回收判定；等待 `pid=None` 后重开。
  没有直接清除进程或伪造恢复成功状态。
- 后续 health ping 由测试 observer 计数，setter 测试等待 4 个真实后台 ping。
  慢消费者通过 Mailbox 原有 Condvar 等队列填满，再开始消费。
- 一般就绪/终态兜底统一 `POLARIS_TEST_WAIT_MS=30000`。
  两端校验为 1000–300000 范围内整数；Rust 使用 `Instant`，Node 轮询使用 `performance.now()`。
  Node 轮询保存真实 timer，避免测试的虚拟 Date/timer 冻结等待截止。
  30 秒是原 5 秒的 6 倍，给并行 SDK/Node 启动留预算；正常事件一到就返回，不固定等 30 秒。
  背压测试需要故意耗尽 request deadline，因此现在约需 30 秒，属于明确的验证成本。
- 只有测试配置改变。`agentd/src/` 未改；Rust 生产文件内新增内容均受 `cfg(test)` 约束。
  未改变发布构建的协议、idle 算法、请求超时默认值或用户数据格式。

配置切换示例（从对应目录执行）：

```sh
# agentd/；未设置时使用 30000ms
POLARIS_TEST_WAIT_MS=60000 npm test
# src-tauri/
POLARIS_TEST_WAIT_MS=60000 cargo test -p polaris-kernel --lib
```

非法值直接使测试失败，不静默回退。策略边界本身使用的短时间（例如故意让 ping 不响应）
与等待成功的兜底不同，不统一改成 30 秒。

## 改动文件与同目录排查清单

以下 Node 路径均相对 `agentd/test/`。

| 文件 | 原依赖 / 本次行为 |
| --- | --- |
| `synchronization.ts`（新增） | 事件等待、可校验的共享预算、带诊断的 Promise 截止 |
| `synchronization.test.ts`（新增） | 验证早到/晚到事件、提前关闭、已缓冲终态、截止失败和非法配置 |
| `helpers.ts` | 固定 5s 改可配置单调时钟兜底；usageDir 放入每个 harness 临时目录 |
| `e2e.test.ts` | JSONL 回调触发等待；退出信号改 close；provider store/usageDir 单测隔离 |
| `fixtures/preload.ts` | 模型只由 abort 释放，移除 10s 自动成功的竞争出口 |
| `sdk-contract.test.ts` | 移除 50ms 盲等和 5s 自动完成；等待模型函数进入再 abort |
| `concurrency.test.ts` | 移除 20ms sleep 和 100ms 调度耗时断言；明确验证 A 未释放时 B/ping/models 已完成，shutdown 已拒绝新请求但尚未完成 |
| `sessions.test.ts` | 慢模型仅由 abort 释放；idle 测试推进虚拟时钟 30/60ms 并观察真实 registry 状态 |
| `timeouts.test.ts` | 工具进入握手后逐步推进虚拟策略时钟；SDK progress 在事件循环中送达后再推进，继续验证 40 次更新及 idle/max/completed 终态 |
| `sandbox-status.test.ts` | probe 超时和缓存 TTL 用虚拟时钟；仅推进 probe 预算即应失败，保持区别于 tool timeout 的断言 |
| `provider-credentials.test.ts` | 移除 20ms fetch 延迟；等两个 store 都尝试修改且首次 refresh 已进入后再释放网络结果 |
| `credential-lock.test.ts` | 移除 10ms 等待租约过期；直接回写测试 owner 的 acquiredAt；进程 IPC 等待沿用明确 ready/go/held 信号，外层预算加宽 |
| `protocol.test.ts` | writer 背压用下一轮事件循环释放 callback，不依赖 2ms；仍等 flush 并校验完整顺序与脱敏 |

以下 Rust 路径均相对 `src-tauri/crates/polaris-kernel/src/chat/backend/`。

| 文件 | 本次行为 |
| --- | --- |
| `pi/test_support.rs`（新增）、`pi/mod.rs` | 测试同步模块、预算校验、状态等待、idle/ping 控制辅助 |
| `pi/client.rs` | 仅测试编译的 `TestAgeIdle`/`TestWaitPings` 控制消息 |
| `pi/supervisor.rs` | 仅测试编译的 idle 年龄推进及后台 ping 计数，不改变生产状态路径 |
| `pi/supervisor_tests.rs` | 既有 State 构造适配测试 observer 字段；保留用户原测试内容 |
| `pi/tests.rs` | idle 前置步骤握手；满队列同步；就绪、崩溃重启、活动请求、熔断、退出、终态等待的 2–5s 兜底统一；随机 session map 和独立专家路径 |
| `pi/experts_recovery_tests.rs` | 移除本地 5s helper；150ms idle 改单 actor 年龄推进；180ms sleep 改等待 4 次后台 ping |
| `pi/extension_tests.rs`、`pi/wiki_tests.rs` | 事件接收的 3/5s fallback 改共享预算；保留所有路由/权限/沙箱断言 |
| `stream.rs` | 仅测试编译的 Condvar 满队列等待 |

保留项也经过检查：

- `agentd/test/web.test.ts` 的 5/20ms 是故意让无响应 adapter/HTTP 请求超时，
  成功条件是命名错误返回，不用 sleep 推断服务就绪；HTTP 已等 listen 回调，端口使用 0。
- `credential-lock.test.ts` 的 60ms 是验证活锁不能被抢占，等待 acquire 明确返回 Busy。
- `sessions.test.ts` 的 40ms 是故意触发 turn idle，随后等真实终态，不是假设模型已经启动。
- Pi `tests.rs` 的 80ms ping-hang、100ms abort 和 300/650ms turn 策略是被测行为。
  timed wire fixture 的 25ms interval 生成真实 reasoning/usage 事件，
  仍等实际终态且保留耗时下限；兜底接收加宽，重复终态检查加强为 `StreamError::Closed`。
- `pi/routes.rs` 的 1s recv 都在同线程先入队后接收；idle/max 边界直接传入构造的 Instant。
  `pi/client.rs` 的 1ms request 是本地 channel 的 timeout/Disconnected 错误区分测试，无 Node 启动。
- 所有留下的 Rust 5/10ms sleep 均位于带截止的状态谓词轮询中，不再作为就绪成功的依据。

文档改动：本文件、[验证报告](f1-validation.md)、`docs/feature-tree.md` F1 入口。
