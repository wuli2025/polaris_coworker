已产出 P6c 部署包，未连接用户服务器、未部署，未执行 commit/push。**部署就绪状态为 BLOCKED：缺少审核后的成品镜像 digest，且本机无法运行 Docker。**

改动包括：

- [部署包与中文说明](/home/mi/work/polaris-pi/deploy/singapore/README.md)：Compose、配置模板、relay 构建文件、密钥初始化、备份恢复、升级、自检和客户端配置模板。
- [功能树入口](/home/mi/work/polaris-pi/docs/feature-tree.md)：已更新，保留其他模块改动。
- [逐文件行为、实际命令与阻塞记录](/home/mi/work/polaris-pi/deploy/singapore/VALIDATION.md)。

拓扑：

```text
id → 可选橙云 → HAProxy → Caddy HTTPS → 内网 authority
relay → 灰云 DNS → HAProxy TLS 透传 → iroh-relay
relay UDP → 直接映射 iroh-relay
```

| 公网端口 | 用途 |
|---|---|
| TCP 80 | ACME HTTP-01、relay HTTP 探测 |
| TCP 443 | 权威 HTTPS、relay TLS 长连接 |
| UDP 7824 | relay QUIC 地址发现 |

relay 选择 **1.0.1**，与仓库 `Cargo.lock` 的 iroh/iroh-relay 同版；官方源码归档 SHA-256 已核对一致。成品镜像 digest 未编造，必须由发布负责人补齐。

实际验证：

- `python3 -m unittest discover -s deploy/singapore/tests -p 'test_*.py' -v`：**19 项通过**。
- 所有脚本 `bash -n`、YAML/TOML 静态检查及文件长度检查通过。
- `docker compose … config --quiet`：WSL Docker 不可用，未通过原生验证。
- shellcheck 未安装；镜像构建、证书、SMTP、公网 smoke 和真实恢复演练均未运行。

待提供：域名、ACME 邮箱、服务器架构与网络条件、两个审核镜像 digest、SMTP/注册策略、测试账号、加密备份方案及客户端发布负责人。最终完成判定交由 Claude 重跑、Pi 独立审查。