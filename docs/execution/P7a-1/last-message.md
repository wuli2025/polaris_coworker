已落地部分脚本，**P7a 仍为 BLOCKED**。

- 新增 `drills/` 执行器、配置、测试和 [运行手册](/home/mi/work/polaris-pi/drills/README.md)；更新功能树，按职责拆出 P2.5 详表。
- 实际验证：`bash -n`、`node --check`、Node 单测 15/15、Python 单测 4/4 通过；27 个文件满足行数及行宽限制。
- 未运行真实模型、Docker、age、产品构建或全量回归。
- 未完成：steer/followUp 服务端入口、产品 Docker 沙箱链，以及 D4 跨主机完整流程；详见 [阻塞清单](/home/mi/work/polaris-pi/docs/drills/P7a-blockers.md)。

完整改动文件、命令和真实结果见 [执行报告](/home/mi/work/polaris-pi/docs/drills/P7a-report.md)。未改产品代码，未提交或推送；保留现有用户改动。