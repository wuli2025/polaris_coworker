P5a 执行状态：**BLOCKED，部分实现已落地**。未提交、未推送。

- 已实现 Node 下载校验、agentd 生产打包、默认路径解析、Pi doctor、旧后端动作守卫、Docker 配置和健康字段；超长文件已拆分。
- agentd 生产包 **123,498,400 字节（117.78 MiB）**，包含构建产物、配置和生产依赖。SDK 契约、WASM、Linux 原生模块检查通过。
- 两壳 `cargo check --offline`、agentd typecheck、vue-tsc 通过；Vitest **29 项通过**。
- kernel 排除报告列出的 6 个 TCP 沙箱项后：**186 单测＋8 集成测试通过**；4 个 Windows 路径基线失败已修复。
- agentd `npm test`：**23/25 文件通过**，剩余为 TCP 绑定和 Node 子进程管道问题，未计为通过。

阻塞：Windows `shellPath` 接线超出任务白名单，尚未获授权；Node 官方下载受代理限制；Docker、Windows/macOS 实跑未完成。[Pi 官方 shell 配置依据](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/windows.md)。

完整命令、体积组成、未完成项及 P5c 文案/类型配合点见[执行报告](/home/mi/work/polaris-pi/docs/pi-migration/p5a-runtime.md)；51 个改动文件及行数见[文件清单](/home/mi/work/polaris-pi/docs/pi-migration/p5a-files.md)。已更新 `docs/feature-tree.md`，待 Claude/Pi 独立复核。