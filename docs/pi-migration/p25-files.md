# P2.5 改动文件

所有路径相对仓库根；含空行、注释。未包含用户已有的 docs/execution/P2.5-1/。

| 文件 | 物理行数 | 行为 |
| --- | ---: | --- |
| `agentd/config/agent.default.json` | 77 | 默认工具档位、附件限额与格式 |
| `agentd/src/config/load.ts` | 42 | 默认档存在性校验，兼容旧 tools 到 default 档 |
| `agentd/src/config/schema.ts` | 50 | 工具档位及附件配置类型和边界校验 |
| `agentd/src/daemon.ts` | 116 | 分发 configure，附件具名错误，ping 版本 |
| `agentd/src/protocol/schema.ts` | 44 | v2 唯一 wire schema、版本常量、会话选项及图片类型 |
| `agentd/src/protocol/version.ts` | 10 | 读取 agentd 元数据与 Pi SDK VERSION |
| `agentd/src/sessions/attachments.ts` | 32 | 图片 MIME/base64、数量及字节上限校验 |
| `agentd/src/sessions/create.ts` | 61 | 逐会话默认/显式设置，注册工具全集并选择当前档位 |
| `agentd/src/sessions/options.ts` | 37 | 调用真实 SDK 配置接口、验证实际激活工具 |
| `agentd/src/sessions/registry.ts` | 228 | 空闲配置锁、图片 prompt 传递及模型支持检查 |
| `agentd/test/protocol-extension.test.ts` | 142 | 6 项 schema、共享样例、配置、忙碌、faux 图片与限额测试 |
| `docs/feature-tree.md` | 234 | v2 入口、依赖、默认值、迁移、验证及阻塞 |
| `docs/pi-migration/p25-files.md` | 40 | 本次逐文件物理行数与行为清单 |
| `docs/pi-migration/p25-result.md` | 134 | 逐项结果、SDK 出处、命令、配置示例及完整阻塞报告 |
| `docs/pi-migration/p25-validation/agentd-tests.log` | 98 | 实际执行命令的完整验证日志 |
| `docs/pi-migration/p25-validation/app-check.log` | 47 | 实际执行命令的完整验证日志 |
| `docs/pi-migration/p25-validation/backend-tests.log` | 82 | 实际执行命令的完整验证日志 |
| `docs/pi-migration/p25-validation/config-tests.log` | 52 | 实际执行命令的完整验证日志 |
| `docs/pi-migration/p25-validation/e2e-diagnostic.log` | 26 | 实际执行命令的完整验证日志 |
| `docs/pi-migration/p25-validation/extension-tests.log` | 46 | 实际执行命令的完整验证日志 |
| `docs/pi-migration/p25-validation/kernel-tests.log` | 291 | 实际执行命令的完整验证日志 |
| `docs/pi-migration/p25-validation/server-check.log` | 48 | 实际执行命令的完整验证日志 |
| `docs/pi-migration/p25-validation/typecheck.log` | 4 | 实际执行命令的完整验证日志 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/bridge.rs` | 70 | 传递现有每轮模式模型选择 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/extension_tests.rs` | 75 | 握手熔断、实际模型/档位/图片请求和版本类型测试 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/input.rs` | 45 | 模型 ID 唯一解析、权限映射、限长读取图片并编码 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/mod.rs` | 143 | 公开 configure_session，逐轮附件请求，移除进程级模型限制 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/protocol.rs` | 144 | Rust v2 类型、ping 验证 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/sessions.rs` | 64 | open/configure 每轮更新设置和映射缓存 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/supervisor.rs` | 310 | 有界握手等待队列、版本不符立即熔断、状态版本 |
| `src-tauri/crates/polaris-kernel/src/chat/backend/pi/tests.rs` | 402 | 旧测试 fixture 升 v2；背压测试先完成握手 |
| `src-tauri/crates/polaris-kernel/tests/fixtures/fake-agentd.mjs` | 88 | v2 ping/configure 及请求记录测试入口 |
| `src-tauri/crates/polaris-kernel/tests/fixtures/protocol-samples.jsonl` | 26 | 保留缺省样例，补 v2 完整字段样例 |
| `src-tauri/crates/polaris-kernel/tests/fixtures/protocol-schema.sha256` | 2 | 更新 schema 哈希（events 哈希未变） |
