# P6c 第 1 轮 Pi 审查处置（Claude）

原始报告：`docs/reviews/P6c-round1b/`（`P6c-round1/` 因误含 `.pyc` 二进制文件 BLOCKED，清理后重送）。主控用**固定版本的真实镜像**验证：用部署包自身 `config.prepare` 渲染配置（测试夹具输入），再在本机 Docker 中运行。

| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 严重 | Caddy `fallback_policy reject` 位置错误，实际会接受无 PROXY 头的连接 | **实测驳回** | `caddy:2.10.2-alpine` 执行 `caddy validate` 返回 `Valid configuration`；`caddy adapt` 输出中该项被解析进 `proxy_protocol` 监听包装器：`{"allow":[...],"fallback_policy":"REJECT","timeout":5000000000,"wrapper":"proxy_protocol"}`。 |
| 2 严重 | HAProxy `mode tcp` + `send-proxy-v2` 与 Caddy 期望时机不符 | **实测驳回** | Caddy 监听包装器顺序为 `['proxy_protocol','tls']`（先读 PROXY 头再 TLS），与 HAProxy TCP 模式在连接开始即发送 PROXY v2 头一致；`haproxy:3.2.26-alpine` `haproxy -c` 退出码 0（仅提示容器名在 compose 网络外无法解析，属预期）。 |
| 3 严重 | `restore.sh` 恢复后未校验指纹 | **采纳** | 恢复是不可逆的身份操作；脚本须在重启后自动比对 `EXPECTED_FINGERPRINT`，不符则停止 authority 并非零退出。 |
| 4 中 | relay 健康检查端点 `/generate_204` 可能不存在 | **有证据驳回** | `iroh-relay-1.0.1/src/server.rs:16,824,836` 明确提供 `/generate_204`。 |
| 5 中 | 备份明文含签名私钥 | **采纳** | 加密作为默认：配置了加密命令/收件人（如 `age` 或 `gpg` 公钥，服务器侧工具，不引入仓库依赖）时生成加密包并删除明文；未配置时脚本拒绝执行，除非显式设置"我已确认备份目录为加密介质"的变量，并在输出中醒目提示。 |

额外（主控）：Caddy 提示 `header_up X-Forwarded-For` 多余、Caddyfile 未格式化——低，顺手清理。

## 第 2 轮（复审 `P6c-round2b/`；首次 `P6c-round2/` 因 Pi 调用超时 BLOCKED，放宽超时后重送）
| # | Pi 结论 | 处置 | 证据 / 理由 |
| --- | --- | --- | --- |
| 1 严重 | `restore.sh` curl 超时参数语义错，可能等 900 秒 | **降为低并采纳** | `--retry-max-time` 限制重试总时长，最坏约为配置值的 2 倍，Pi 夸大；但与"校验超时"语义不符，改为单次请求超时取较小值、总时长受配置值约束。 |
| 2 中 | gpg 解密可能挂起 | **降为低并采纳** | `--batch --no-tty` 下缺口令通常直接失败；仍加 `subprocess` 超时（可配置）作纵深防御，超时报具名错误并走既有清理。 |
| 3 中 | `Path.unlink()` 删不掉指向目录的符号链接 | **实测驳回** | 主控实测：对指向目录的符号链接调用 `Path.unlink()` 后链接被删除、目标目录保留。 |
结论：两项低修复后 P6c **标记完成**（2 轮上限，无阻塞）。
