# 任务 R4：P3b 审查意见修复

先读 `docs/reviews/P3b2-round1-triage.md`。修复：C2（高）、C3（中）、#2、#5、#7、#9。驳回项不要改代码。

- C2：`polaris-sandbox/src/policy.rs` 删除基于名字的全树扫描与组件名匹配；保留受保护路径双向前缀判定、规范化、禁止挂载 `$HOME` 本身及其祖先；路由前校验不做目录遍历。测试：普通项目含 `src/data/`、`sessions/`、`fixtures/x.key` 可挂载；项目位于受保护路径之内或包含受保护路径时拒绝；`$HOME`、`/` 拒绝；校验调用不随项目文件数增长（例如构造大量文件后校验仍不读目录）。
- C3：修掉 `model_profile_image_and_idle_reconfiguration_are_on_the_wire` 的时序依赖（等待确定性信号，不靠"马上读文件"与固定时长轮询）；本地连续全量运行 `cargo test -p polaris-kernel --lib` 至少 10 次无该测试失败，在结果中给出运行次数与结果。
- #2：专家加载失败错误包含选择类型与 id（不含提示词）。
- #5：`src/features/sandbox/` 前端对目录输入做即时绝对路径格式提示（POSIX 与 Windows 盘符），最终以后端为准。
- #7：随 C2 补端到端挂载测试（见上）。
- #9：`path_for_docker` 补注释。

允许修改：`src-tauri/crates/polaris-sandbox/`、`src-tauri/crates/polaris-kernel/src/chat/backend/`（含测试与 fixtures）、`src-tauri/crates/polaris-kernel/tests/`、`src/features/sandbox/`、`docs/`。
无新依赖；≤500 行/≤500 字符；新 Rust 代码无 unwrap/expect。
必须通过：两壳 `cargo check --offline`；`cargo test -p polaris-sandbox --offline`；`cargo test -p polaris-kernel --lib --tests --offline`（基线与沙箱 TCP 项除外并列出）；`npx vue-tsc --noEmit`；`npx vitest run`。不 commit。
