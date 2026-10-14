已执行 C2、C3、#2、#5、#7、#9，驳回项未改，未提交或推送。

- 沙箱策略删除名称匹配和目录遍历，保留路径保护；补挂载回归测试及 Windows 注释。
- 专家加载错误包含 kind/id，不暴露提示词。
- C3 改用确定性同步；前端增加即时绝对路径提示。
- 已更新功能树；全部修改满足 500 行/500 字符限制，无新依赖、新增 Rust unwrap/expect。

实际验证：

- 两壳 `cargo check --offline`：通过。
- sandbox：17 项通过。
- kernel lib 连续全量 **10 次，C3 均通过**；每次仍有相同的 4 项基线、6 项 TCP 限制失败。
- 排除上述许可例外后：171 单测、4 集成测试通过。
- `vue-tsc`、Vitest：通过，前端 25 项。

完整改动文件、命令、失败名单及日志见 [R4 执行报告](/home/mi/work/polaris-pi/docs/pi-migration/r4-review-fixes.md)。真实 Docker、Windows 实机及浏览器点击未验证；无新增范围阻塞。最终完成状态由 Claude / Pi 复核。