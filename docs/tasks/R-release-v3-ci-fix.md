# R-release-v3-ci-fix：修复 v3.0.0 发布 CI 的 Windows 测试失败

背景：`release` 工作流的 "Verify tagged source" 在 **windows-latest** 上跑 `cargo test --lib`，107 过 1 败，导致整个发布（Windows/Mac 安装包）被拦下。同一测试在 Linux 通过。macOS 编译错误和 Docker 版本号钉死两项我（Claude）已修好并提交，**不要动**。

## 失败的测试
`src-tauri/src/mesh/tls_tests.rs` → `mesh::tls_tests::ordinary_and_watch_clients_share_private_ca_policy`

CI 日志（Windows）：
```
panicked at src\mesh\tls_tests.rs:74:13:
assertion `left == right` failed
  left: false
 right: true
```
第 74 行是循环里的 `assert_eq!(client.get("https://localhost:{port}").call().is_ok(), trusted)`。即 `trusted == true`（配置了 `authority.extraCaPem` 指向私有 CA）时，客户端访问 `https://localhost:<port>` **失败**了；`trusted == false` 时的失败预期那一支是对的。前面的 `openssl` 命令行调用（生成 CA、服务器证书）都成功了（它们用了 `?`），所以 CI 的 Windows 机器上 openssl 可用。

我这台 Windows 开发机上没有 openssl.exe，无法本地复现；你在 WSL 沙箱里也没法跑 Windows。因此需要你**读代码推断根因**，并让下一轮 CI 即使仍失败也能直接给出答案。

## 任务
1. 通读被测路径：`src-tauri/crates/polaris-collab/src/collab/interconnect/http_client.rs`（根证书构建：webpki_roots + extraCaPem）、`src-tauri/src/mesh/`（`transport::agent()`、`security::watch_agent()`）、该测试本身。重点怀疑（逐条判断，不要全信）：
   - Windows 上 `localhost` 先解析到 `::1`，而测试服务只绑定 `127.0.0.1`；
   - Windows 上的 openssl（可能是 Git for Windows / Strawberry Perl 自带的 1.1.x/3.x，**默认没有 openssl.cnf**）生成出来的 CA/叶子证书缺少扩展（如 SKI/AKI、keyUsage），被 rustls-webpki 拒绝；
   - ureq/rustls 在 Windows 的证书校验器、代理环境变量（`HTTPS_PROXY` 在 CI 里可能不同）、时间/有效期（`-days 1`）；
   - `extraCaPem` 路径含反斜杠或被当成内联 PEM 的判定问题（见 `http_client.rs:35` 附近）。
2. 若根因是**产品代码**的 Windows 差异（例如路径判定、根证书合并），修产品代码并补回归测试。
3. 若根因是**测试夹具**（证书生成方式、`localhost` 解析），把测试改得跨平台确定：优先不依赖外部 `openssl` 命令（若仓库依赖里已有 `rcgen` 或等价 crate，可在 `[dev-dependencies]` 里使用；**不要新增生产依赖**）；或显式生成含 `subjectKeyIdentifier`、`authorityKeyIdentifier`、`keyUsage` 的证书，并用 `127.0.0.1`/IP SAN 或自定义解析避免 `::1`。
4. **无论如何**，把第 74 行的断言改为失败时输出完整错误链（`trusted`、`probe` 的 Err 内容、使用的 openssl 版本字符串），这样 CI 再败也能看到真实原因。
5. 不得削弱测试要表达的三条语义：配了 CA 才能连、没配必须拒绝、主机名不匹配必须拒绝（`InterconnectTlsCertificateError`）。不得用 `#[ignore]` / `cfg(windows)` 跳过测试。

## 约束
- 单文件 ≤500 行、单行 ≤500 字符；不新增生产依赖；不改 `mobile/`；不 commit、不 push；不要修改 `docker/`、`scripts/`、`.github/`。
- 验证：在沙箱外 Claude 会重跑；你在沙箱内尽力跑 `cargo test --offline -p polaris-app --lib mesh::tls_tests`（Linux 上必须仍通过）和 `cargo check --offline -p polaris-app`。
- 报告写入 `docs/tasks/R-release-v3-ci-fix-report.md`：根因判断（写明哪些是推断、哪些已被代码证实）、改动文件、验证结果、仍不确定之处。
