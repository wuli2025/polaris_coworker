# 审查 brief：P4 提供商与凭据迁移到 Pi

两包：A = agentd 凭据/模型 + Rust provider 模块中的新逻辑（`P4a-diff.patch`；`store.rs` 拆分出的纯搬移文件已排除——主控核对旧 `store.rs` 的 67 个函数全部保留在新文件中）；B = 其余 Rust/前端接线（`P4b-diff.patch`）。测试文件已排除。

## 目标与验收
1. 提供商翻译：每个 Polaris 提供商（原为给 claude CLI 的环境变量 `settings_config.env`）翻译成 agentd provider 配置（baseUrl、api 类型、模型清单）。
2. 凭据单一来源（方案 A）：agentd 与 Rust 共享 `providers.json` 的 credentials，`mkdir` 目录锁覆盖"读-刷新-写回"整个事务，原子写（临时文件 + rename，0600）；明文不进日志/事件；刷新后的 token 持久化；并发刷新互不覆盖。
3. 切换：向 agentd 下发配置；空闲会话自动重开保留历史，运行中会话本轮后生效；Pi 后端下"联动模式"不再写用户的 `~/.claude/settings.json`。
4. 登录：优先复用 Pi 原生 OAuth，凭据落共享存储。
5. 用量：Pi usage 落库，与旧 `~/.claude/projects/**/*.jsonl` 合并去重。
6. 故障切换：Pi 错误分类（MODEL_AUTH/RATE_LIMIT/...）接入 `polaris-failover`。

## Claude 实测（沙箱外）
agentd typecheck 通过、`npm test` 107/107；Rust 两壳 `cargo check` 通过（7 警告同基线）；`cargo test -p polaris-kernel --lib --tests` 连续 3 次 181 通过、4 失败均为基线；vitest 25/25、vue-tsc 通过；改动文件 ≤500 行。

## 主控已发现（请独立评估其严重性与修法，不必复述现象）
- 锁的注释为"Never steal a lock from a slow refresh"：持锁进程崩溃后 `.lock` 目录永久残留，之后所有凭据读写超时，需人工删除。

## 请重点审查
锁的正确性（崩溃残留、跨平台 `mkdir` 语义、超时与重试、锁内是否可能有阻塞网络调用导致长时间持锁）；原子写在 Windows 上 rename 覆盖的行为；明文凭据是否可能出现在日志、事件、错误消息、前端；OAuth 刷新并发（两进程同时发现过期）；切换时会话重开是否丢历史；用量去重键是否会误合并或漏合并；故障切换是否会因 Pi 的可重试错误误判提供商故障。
