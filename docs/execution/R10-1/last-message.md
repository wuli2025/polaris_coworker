R10 改动已落地：

- 恢复后自动比对指纹；不符停机并失败，缺配置默认拒绝。
- 默认 age/gpg 加密备份，支持解密恢复及明文清理；例外须显式确认并告警。
- 清理 Caddy 多余头、整理格式；更新测试、README 和功能树。驳回项未改。

验证：指定 unittest 命令 **29 项通过**；全部 8 个脚本 `bash -n` 通过；行数限制与 `git diff --check` 通过。

**原生验证 BLOCKED**：缺 age/Caddy，Docker socket 拒绝访问，gpg-agent 启动失败；未完成真实加解密及 Caddy fmt/validate 验证。

完整文件清单、实际命令和结果见 [执行报告](/home/mi/work/polaris-pi/docs/execution/R10/report.md)。未提交或推送，保留用户改动；最终完成状态由 Claude 决定。