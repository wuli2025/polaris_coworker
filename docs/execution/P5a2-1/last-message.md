代码改动已落地，**验收状态：BLOCKED**。

- 接通 Windows `shellPath`、MinGit PATH 与 doctor 回落诊断。
- 清理 doctor 警告，保留受旧后端守卫保护的 Node 安装链。
- Node 下载源改为官方 → R2 → 可配置 Node 镜像，移除 GitHub 代理。

验证结果：

- agentd typecheck、vue-tsc 通过；Vitest 32 项、runtime 7 项通过。
- agentd 测试 24/26 文件通过；另两项受 TCP 监听限制、Node 管道异常阻塞。
- Kernel 排除已列明的 6 个 TCP 沙箱项后，189 单元测试及 8 集成测试通过。
- 两壳 check 成功，但警告数实际为 **0 / 4**，不满足 brief 指定的各 7 条。
- 未实跑 Windows/macOS shell、真实下载及 Docker 验证。

[完整执行报告](/home/mi/work/polaris-pi/docs/pi-migration/p5a2-closeout.md) 已列出全部改动文件、命令、失败项与阻塞原因；[功能树](/home/mi/work/polaris-pi/docs/feature-tree.md) 已更新。23 个文件符合行数/行长限制，无新依赖，未提交或推送，保留用户已有改动。