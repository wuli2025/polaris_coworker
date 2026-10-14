# R5 改动文件清单

只列本执行者改动；排除会话开始已有的 `docs/execution/R5-1/`、`tests/`。

| 文件 | 行数 | 行为 |
| --- | ---: | --- |
| [agentd/src/daemon.ts](../../../agentd/src/daemon.ts) | 132 | 用量失败日志/计数、ping、目录响应类型 |
| [agentd/src/log.ts](../../../agentd/src/log.ts) | 15 | 结构化日志计数字段 |
| [agentd/src/main.ts](../../../agentd/src/main.ts) | 76 | 最大锁持有时长配置 |
| [agentd/src/models/credential-lock.ts](../../../agentd/src/models/credential-lock.ts) | 65 | 凭据代次锁/进程身份或目录 apply 重建 |
| [agentd/src/models/file-credentials.ts](../../../agentd/src/models/file-credentials.ts) | 86 | 凭据代次锁/进程身份或目录 apply 重建 |
| [agentd/src/models/lock-owner.ts](../../../agentd/src/models/lock-owner.ts) | 43 | 凭据代次锁/进程身份或目录 apply 重建 |
| [agentd/src/models/runtime.ts](../../../agentd/src/models/runtime.ts) | 51 | 凭据代次锁/进程身份或目录 apply 重建 |
| [agentd/src/protocol/schema.ts](../../../agentd/src/protocol/schema.ts) | 75 | v6 schema（目录含阶梯定价）、ping 计数 |
| [agentd/src/sessions/registry.ts](../../../agentd/src/sessions/registry.ts) | 394 | 用量写盘失败回调 |
| [agentd/test/catalog.test.ts](../../../agentd/test/catalog.test.ts) | 29 | 新增/调整行为测试与 FileCredentials 注入 |
| [agentd/test/credential-lock.test.ts](../../../agentd/test/credential-lock.test.ts) | 86 | 新增/调整行为测试与 FileCredentials 注入 |
| [agentd/test/fixtures/credential-lock-child.ts](../../../agentd/test/fixtures/credential-lock-child.ts) | 19 | 新增/调整行为测试与 FileCredentials 注入 |
| [agentd/test/helpers.ts](../../../agentd/test/helpers.ts) | 58 | 新增/调整行为测试与 FileCredentials 注入 |
| [agentd/test/provider-credentials.test.ts](../../../agentd/test/provider-credentials.test.ts) | 108 | 新增/调整行为测试与 FileCredentials 注入 |
| [agentd/test/provider-login.test.ts](../../../agentd/test/provider-login.test.ts) | 41 | 新增/调整行为测试与 FileCredentials 注入 |
| [agentd/test/provider-usage.test.ts](../../../agentd/test/provider-usage.test.ts) | 50 | 新增/调整行为测试与 FileCredentials 注入 |
| [docs/execution/R5-fixes/logs/agentd-tests.log](../../../docs/execution/R5-fixes/logs/agentd-tests.log) | 176 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/desktop-check.log](../../../docs/execution/R5-fixes/logs/desktop-check.log) | 47 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/e2e-baseline.log](../../../docs/execution/R5-fixes/logs/e2e-baseline.log) | 26 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/e2e.log](../../../docs/execution/R5-fixes/logs/e2e.log) | 26 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/focused-agentd.log](../../../docs/execution/R5-fixes/logs/focused-agentd.log) | 40 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/kernel-all.log](../../../docs/execution/R5-fixes/logs/kernel-all.log) | 354 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/kernel-filtered.log](../../../docs/execution/R5-fixes/logs/kernel-filtered.log) | 263 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/kernel-tests.log](../../../docs/execution/R5-fixes/logs/kernel-tests.log) | 331 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/lock-interop.log](../../../docs/execution/R5-fixes/logs/lock-interop.log) | 51 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/new-tests.log](../../../docs/execution/R5-fixes/logs/new-tests.log) | 41 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/server-check.log](../../../docs/execution/R5-fixes/logs/server-check.log) | 82 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/typecheck.log](../../../docs/execution/R5-fixes/logs/typecheck.log) | 4 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/vitest.log](../../../docs/execution/R5-fixes/logs/vitest.log) | 17 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/vue-tsc.log](../../../docs/execution/R5-fixes/logs/vue-tsc.log) | 0 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/logs/web.log](../../../docs/execution/R5-fixes/logs/web.log) | 37 | 执行报告与实际验证日志 |
| [docs/execution/R5-fixes/report.md](../../../docs/execution/R5-fixes/report.md) | 101 | 执行报告与实际验证日志 |
| [docs/feature-tree.md](../../../docs/feature-tree.md) | 356 | 入口、依赖、配置、迁移、验证索引 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/mod.rs](../../../src-tauri/crates/polaris-kernel/src/chat/backend/mod.rs) | 161 | IPC 代次检查、协议/计数同步、测试握手更新 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/mod.rs](../../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/mod.rs) | 185 | IPC 代次检查、协议/计数同步、测试握手更新 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/process.rs](../../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/process.rs) | 166 | IPC 代次检查、协议/计数同步、测试握手更新 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/protocol.rs](../../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/protocol.rs) | 225 | IPC 代次检查、协议/计数同步、测试握手更新 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/supervisor.rs](../../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/supervisor.rs) | 322 | IPC 代次检查、协议/计数同步、测试握手更新 |
| [src-tauri/crates/polaris-kernel/src/chat/backend/pi/tests.rs](../../../src-tauri/crates/polaris-kernel/src/chat/backend/pi/tests.rs) | 406 | IPC 代次检查、协议/计数同步、测试握手更新 |
| [src-tauri/crates/polaris-kernel/src/provider/claude_login.rs](../../../src-tauri/crates/polaris-kernel/src/provider/claude_login.rs) | 346 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/codex_login.rs](../../../src-tauri/crates/polaris-kernel/src/provider/codex_login.rs) | 419 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/credential_file.rs](../../../src-tauri/crates/polaris-kernel/src/provider/credential_file.rs) | 38 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/credential_lock.rs](../../../src-tauri/crates/polaris-kernel/src/provider/credential_lock.rs) | 85 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/credential_lock_tests.rs](../../../src-tauri/crates/polaris-kernel/src/provider/credential_lock_tests.rs) | 67 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/lock_owner.rs](../../../src-tauri/crates/polaris-kernel/src/provider/lock_owner.rs) | 65 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/mod.rs](../../../src-tauri/crates/polaris-kernel/src/provider/mod.rs) | 108 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/pi-default.json](../../../src-tauri/crates/polaris-kernel/src/provider/pi-default.json) | 11 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/pi_config.rs](../../../src-tauri/crates/polaris-kernel/src/provider/pi_config.rs) | 35 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/selection.rs](../../../src-tauri/crates/polaris-kernel/src/provider/selection.rs) | 11 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/store/commands.rs](../../../src-tauri/crates/polaris-kernel/src/provider/store/commands.rs) | 181 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/src/provider/store/persistence.rs](../../../src-tauri/crates/polaris-kernel/src/provider/store/persistence.rs) | 186 | 锁/配置/选择代次或异步命令 |
| [src-tauri/crates/polaris-kernel/tests/catalog_protocol.rs](../../../src-tauri/crates/polaris-kernel/tests/catalog_protocol.rs) | 21 | 共享向量/样例/哈希、假 daemon、集成测试 |
| [src-tauri/crates/polaris-kernel/tests/credential_lock_interop.rs](../../../src-tauri/crates/polaris-kernel/tests/credential_lock_interop.rs) | 42 | 共享向量/样例/哈希、假 daemon、集成测试 |
| [src-tauri/crates/polaris-kernel/tests/fixtures/credential-lock-vectors.json](../../../src-tauri/crates/polaris-kernel/tests/fixtures/credential-lock-vectors.json) | 151 | 共享向量/样例/哈希、假 daemon、集成测试 |
| [src-tauri/crates/polaris-kernel/tests/fixtures/fake-agentd.mjs](../../../src-tauri/crates/polaris-kernel/tests/fixtures/fake-agentd.mjs) | 117 | 共享向量/样例/哈希、假 daemon、集成测试 |
| [src-tauri/crates/polaris-kernel/tests/fixtures/protocol-samples.jsonl](../../../src-tauri/crates/polaris-kernel/tests/fixtures/protocol-samples.jsonl) | 39 | 共享向量/样例/哈希、假 daemon、集成测试 |
| [src-tauri/crates/polaris-kernel/tests/fixtures/protocol-schema.sha256](../../../src-tauri/crates/polaris-kernel/tests/fixtures/protocol-schema.sha256) | 2 | 共享向量/样例/哈希、假 daemon、集成测试 |
| [src-tauri/crates/polaris-kernel/tests/provider_switch.rs](../../../src-tauri/crates/polaris-kernel/tests/provider_switch.rs) | 77 | 共享向量/样例/哈希、假 daemon、集成测试 |
