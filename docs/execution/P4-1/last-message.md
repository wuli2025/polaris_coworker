P4 实现已落盘，**验收状态：BLOCKED**，未提交或推送。

- 采用方案 A：共享 `providers.json.credentials`，文件锁覆盖刷新与写回；依据 Pi 的 `CredentialStore` / `OAuthCredential` d.ts。
- 已接入提供商翻译、会话切换、原生 OAuth、用量合并、错误分类与故障转移，以及通知发送失败修复。
- 通过：agentd typecheck、两壳离线 check、vue-tsc；排除既有例外后，kernel **175 单测＋4 集成测试通过**。
- 阻塞：agentd 全量 **21 组通过、2 组失败**，分别为 web 监听 EPERM、e2e 首次 ping 超时。
- 前端仍需调整联动/代理开关、登录及用量说明。真实 OAuth、跨进程刷新竞速和跨平台验收未运行。

[完整报告：方案证据、命令结果、迁移与未完成项](/home/mi/work/polaris-pi/docs/pi-migration/p4-result.md)  
[61 个改动文件及行数](/home/mi/work/polaris-pi/docs/pi-migration/p4-files.md)

已更新 `docs/feature-tree.md`；自有文件均满足 500 行／500 字符限制。最终完成与否由 Claude 重跑、Pi 审查后决定。