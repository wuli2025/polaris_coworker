P5b 已实现，未 commit/push，保留了并行用户改动。

- wiki：Pi 走 AgentBackend，cwd 为 `KB/wiki`，使用 auto-current；成功写入才报告页面，取消/超时走 abort。旧 Claude 路径仅在 claude-cli 后端启用。
- 无头调用已更名，旧名保留 deprecated 转发；远程名单已配置化。
- 两壳 `cargo check --offline`：通过，0 警告。
- 三 crate 测试：314 通过、9 忽略；排除 6 个已实测失败的 TCP 沙箱项。fable 0 失败。
- 远程执行测试：12 通过；`npx vue-tsc --noEmit` 通过。

[改动文件、行数、完整命令及结果](/home/mi/work/polaris-pi/docs/pi-migration/p5b-files-and-validation.md)；[行为差异与配置说明](/home/mi/work/polaris-pi/docs/pi-migration/p5b-indirect-callers.md)。

未做真实模型、Docker、Windows/macOS 验证；rustfmt 未安装。测试另有一条既有 fable 警告，已记录。最终完成判定交由 Claude，Pi 独立审查。