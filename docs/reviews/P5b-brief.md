# 审查 brief：P5b 间接调用方迁移
附 `P5b-diff.patch`（wiki/内核/exec，**含测试**）与 `P5b-fable.patch`（fable 改名与两个测试改为跨平台）。
## 验收条件
1. wiki 编译走 AgentBackend：会话 cwd = wiki 根目录，写权限仅限该目录（auto-current），沙箱沿用用户选择；后端事件翻译为既有 `kb:compile` 进度（前端不改）；仅成功的 write/edit 且路径在 wiki 目录内才报告"写入页面"；保留维护锁、写入约束与失败收尾；取消/超时走 abort；旧 claude 路径仅 `agent.backend=claude-cli` 时启用。
2. `run_claude_readonly*` 更名为后端中立名字，旧名 `#[deprecated]` 转发；只改名不改行为。
3. `exec.rs` 远程执行允许名单进配置，默认与现状一致。
4. fable 两个重构前即失败的 Windows 路径测试改为跨平台，生产逻辑不变，Windows 安全回归断言保留在 `cfg(windows)`。
## Claude 实测（沙箱外）
两壳 `cargo check` 0 警告；`cargo test -p polaris-kernel -p polaris-wiki -p polaris-fable --lib --tests` 连续 2 次：内核 201、fable 105 均 0 失败；agentd 117/117；vitest 32/32；vue-tsc 通过。fable 中 7 个既有超长文件本次仅机械改名（各 ±1~6 行），记为既有技术债。
## 请重点审查
wiki 编译能否写出 wiki 目录（符号链接、`..`、绝对路径、子代理、bash 工具间接写）；进度事件在失败/取消时是否仍收尾；维护锁在异常路径是否释放；deprecated 转发是否有遗漏调用点；远程执行名单配置能否被注入扩展（配置来源与校验）。
