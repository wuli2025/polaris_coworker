# R-release-v3-ci-fix 执行报告

本报告是 Codex 执行记录；最终是否完成由 Claude 重跑、Pi 审查后决定。
Windows 原始失败未复现，不能宣称已证实根因或 Windows 发布已恢复。

## 根因判断

- 已证实：旧测试通过系统 OpenSSL 生成证书，未指定 `-config`；CA 仅显式设置 basicConstraints，叶子仅显式设置 SAN、EKU、basicConstraints。其余扩展和默认签名配置依赖安装环境。新夹具显式设置 SHA-256、SKI、AKI、keyUsage 等，消除此项环境差异。
- 推断：证书生成环境差异是值得优先消除的嫌疑，但不能仅凭缺少 SKI/AKI/keyUsage 就断言 rustls-webpki 必然拒绝。原日志只有布尔断言，无证书或错误链，无法确认是哪项导致失败。完全缺失配置文件通常会让 `openssl req` 自身失败，与已知日志不符；“文件存在但默认配置不同”仍可能。
- 已证实：旧服务只监听 IPv4，却请求 localhost。ureq 2.12.1 的 `stream.rs` 会遍历解析地址，连接失败会尝试下一个地址，所以“::1 排第一”本身不足以解释失败；地址列表或超时差异仍无法从日志排除。新测试所有请求直接访问 127.0.0.1，不再依赖名称解析。
- 已证实：`http_client.rs` 配置 CA 时使用 webpki_roots 加额外根、显式 ring provider，未按 Windows 切换系统证书验证器。`transport::agent()` 与 `security::watch_agent()` 均调用此公开接口，无发现产品级 Windows 分支错误。
- 已证实：ureq 默认不读取代理环境变量；本机 `cargo tree` 中 ureq 2.12.1 仅启用 default/gzip/json/tls，没有 proxy-from-env/native-certs。当前源码无启用这些特性的声明，不能把 CI 的 HTTPS_PROXY 直接认定为原因；未取得 Windows 实际特性树。
- 已证实：`certificates()` 只有发现 PEM 起始标记才按内联 PEM 解析，否则直接 `std::fs::read(extra)`，没有路径分隔符判断。测试使用 serde_json 序列化 PathBuf，反斜杠会正确转义；没有发现路径误判的代码证据。
- 时间：原证书现场生成、有效期一天，通常不应刚生成就过期；没有 CI 时钟证据。改为显式两天有效期，仍保留真实有效期校验；无法消除机器时钟突然回拨等情况。

## 改动文件与行为

- `src-tauri/src/mesh/tls_tests.rs`：继续使用真实 ordinary/watch 构造器及配置文件；先验证坏路径被拒绝。两个 IPv4 TLS 服务使用同一 CA 和服务密钥，分别提供 IP SAN 匹配证书与仅 DNS:localhost 的不匹配证书。两客户端各覆盖未信任拒绝、信任成功、名称不匹配拒绝；负例均要求 `InterconnectTlsCertificateError`，防止连接错误冒充证书拒绝。
- 同一测试的断言输出客户端名、trusted、probe 的 Debug 内容、逐层 `Error::source()` 的 Display/Debug 和 OpenSSL 版本。生成失败输出命令参数、版本及进程输出。无 ignore 或平台跳过。
- `src-tauri/src/mesh/tls_fixture.cnf`：测试专用 OpenSSL 模板，显式 CA/叶子扩展、非交互 subject；req 指定配置，x509 指定扩展节，并为子进程显式设置 OPENSSL_CONF，不修改父进程该环境变量。
- `docs/feature-tree.md`：在现有发布条目登记入口、依赖、配置、迁移、验证报告；保持 500 行。
- `docs/tasks/R-release-v3-ci-fix-report.md`：本报告。
- 仓库锁文件无 rcgen；选择任务允许的显式 OpenSSL 方案，未新增任何依赖，未改生产代码、Cargo 文件、mobile、docker、scripts 或 .github。保留任务开始时已有未跟踪文件，未 commit/push。

## 配置与迁移

产品配置不变、无数据迁移。`authority.extraCaPem` 默认 null，接受证书 PEM 字符串或路径，由现有反序列化/配置校验及 PEM/根证书解析验证类型与内容。
例如切换为 `{"authority":{"extraCaPem":"C:\\certs\\private-ca.pem"}}`，恢复默认信任则设为 `{"authority":{"extraCaPem":null}}`；测试每次重建两个 agent 验证切换。
测试证书策略位于 `tls_fixture.cnf`：默认 server 节为 `IP:127.0.0.1`，mismatch 节为 `DNS:localhost`；由测试显式选择节，OpenSSL 验证配置语法，rustls 加载证书并实际握手验证。有效期通过命令显式传入 `-days 2`。
仍要求 PATH 上存在 openssl；无此工具会明确失败，不会跳过测试。

## 实际验证

- 仓库根误运行一次 `cargo test --offline -p polaris-app --lib mesh::tls_tests`：退出 101，根目录无 Cargo.toml；随后在 `src-tauri/` 重跑。
- 在 `src-tauri/` 执行 `cargo test --offline -p polaris-app --lib mesh::tls_tests`：编译成功，运行退出 101，0 passed / 1 failed，`PermissionDenied: Operation not permitted`。增加监听上下文后再次执行，仍退出 101（0 passed / 1 failed），明确报 `TLS fixture server.pem: bind 127.0.0.1:0: PermissionDenied (code 1)`；未达到客户端断言。
- 在 `src-tauri/` 执行 `cargo check --offline -p polaris-app`：退出 0，通过（1m37s，包括等待构建锁）。最后一次修改仅为测试监听错误上下文，不影响非 test 编译。
- `cargo tree --offline -p polaris-app -e features -i ureq@2.12.1`：退出 0，特性判断见上文。
- `openssl version`：OpenSSL 3.0.13 30 Jan 2024。
- Python 临时目录验证：故意将 OPENSSL_CONF 指向缺失文件，用模板显式生成 CA 和两种叶子；`openssl verify -CAfile ca.pem -purpose sslserver -verify_ip 127.0.0.1 server.pem` 返回 0，mismatch.pem 返回 2 / IP address mismatch。脚本整体退出 0，确认配置不依赖机器默认 openssl.cnf；这不是 Windows 或 rustls 测试的替代。
- `git diff --check`：通过。Python 检查本次全部四个文件，每个文件 ≤500 行、每行 ≤500 字符，通过。
- Python `socket.socket()` / `bind(("127.0.0.1", 0))` 独立探测：退出 1，`PermissionError(1, Operation not permitted)`，确认沙箱阻止本地 socket 操作。

## 未完成与不确定性

**BLOCKED：沙箱禁止本地 TCP socket，无法确认 Linux 目标测试通过；当前权限策略不允许申请沙箱外执行。**

没有 Windows 执行环境，未运行 Windows/macOS CI、完整 cargo test --lib 或发布工作流；也未测试 OpenSSL 1.1.x。需要 Claude 在 Windows 重跑目标测试及发布验证。如果仍失败，新断言将提供真实 TLS/网络错误链及 OpenSSL 版本；目前只能报告夹具确定化和诊断增强，不能报告原始根因已证实。
