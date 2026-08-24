# 发版手册（手动建 Release + 打通自动更新）

本仓库的 Actions `GITHUB_TOKEN` **无法创建 Release**（即便 Workflow 权限为 write、
job 声明 `contents: write`，调 create-a-release 仍报 `Resource not accessible by
integration` —— 账号级限制）。所以 `release.yml` 负责**构建、签名、生成并验证
`latest.json`、上传 release-ready artifact**；Release 仍由仓库 owner 在本机用 `gh`
创建。本流程不会自动创建 tag 或生产 Release。

## 1. 触发构建

```powershell
# 版本号需先在 src-tauri/tauri.conf.json 与 package.json 同步好
git tag -a v0.2.12 -m "..."
git push origin v0.2.12     # 触发 release.yml: Windows + macOS 并行构建并签名
```

构建从 **tag 指向的提交树**出包。只能给已经合并并验证过的当前 `origin/main` 提交打 tag；
操作前先 `git fetch origin`，确认本地 `main` 可以快进到且最终与 `origin/main` 一致。
不要从未合并或落后的本地分支创建生产 tag。

## 2. 下载经过校验的 release-ready 产物

```powershell
$runId = (gh run list --workflow=release.yml --limit 1 --json databaseId --jq '.[0].databaseId')
$dir = "D:\polaris\_release"
gh run download $runId -n "release-ready-v0.2.12" -D $dir
```

关键产物：
- Windows: `...\nsis\Polaris_<v>_x64-setup.exe` 和 `.exe.sig`
- macOS:   `...\macos\Polaris.app.tar.gz` 和 `.sig`（更新用）、`...\dmg\Polaris_<v>_universal.dmg`（全新安装用）
- 清单: `...\latest.json`

`latest.json` 由 `scripts/release-manifest.mjs` 从真实安装包与 `.sig` 生成。脚本会拒绝缺失
签名、错误版本、额外/缺失平台和不安全 URL；不要再手工拼 JSON。发布前检查清单包含
`windows-x86_64`、`darwin-x86_64`、`darwin-aarch64`，且版本与 tag 一致。

## 3. 建 Release（owner 身份，标记 latest）

```powershell
gh release create v0.2.12 `
  "$dir\...\Polaris_0.2.12_x64-setup.exe" `
  "$dir\...\Polaris_0.2.12_universal.dmg" `
  "$dir\...\Polaris.app.tar.gz" `
  "$dir\...\Polaris.app.tar.gz.sig" `
  "$dir\...\Polaris_0.2.12_x64-setup.exe.sig" `
  "$dir\latest.json" `
  --repo wuli2025/polaris_coworker --title "Polaris v0.2.12" --notes "……" --latest
```

`--latest` 让 `releases/latest/download/latest.json` 解析到本版 —— 这正是
`tauri.conf.json > plugins.updater.endpoints` 轮询的地址，旧版本据此自动升级。

## 4. 同步 Cloudflare 自托管兜底（必做）

客户端会在 GitHub 代理、Cloudflare 和 GitHub 直连之间自动切换，并始终执行 minisign
校验。Cloudflare 必须同步 `latest.json` 和清单引用的两个更新包：

```powershell
# 把 win 安装器 + mac 自动更新包（注意是 .app.tar.gz，不是 dmg）拷进站点 downloads/，文件名保持与 GitHub 资产一致
Copy-Item "$dir\...\Polaris_0.2.12_x64-setup.exe" "D:\polaris\polaris-site\downloads\"
Copy-Item "$dir\...\Polaris.app.tar.gz"          "D:\polaris\polaris-site\downloads\"
Copy-Item "$dir\latest.json"                      "D:\polaris\polaris-site\downloads\"
# 可选：dmg 供官网手动下载
Copy-Item "$dir\...\Polaris_0.2.12_universal.dmg" "D:\polaris\polaris-site\downloads\"
wrangler pages deploy "D:\polaris\polaris-site" --project-name polaris --commit-dirty=true
```

部署后必须让脚本验证远端字节长度、魔数和完整 SHA-256；这样 Pages 的 HTTP 200 HTML fallback
或任何同长度的损坏文件都不会被误判为安装包：

```powershell
node scripts/release-manifest.mjs `
  --artifacts $dir `
  --version 0.2.12 `
  --repo wuli2025/polaris_coworker `
  --pub-date "2026-08-24T06:00:00.000Z" `
  --out "$dir\verified-latest.json" `
  --remote-base https://polaris-2us.pages.dev/downloads
```

最后分别读取 `llmwiki.cloud`、Cloudflare Pages、GitHub proxy 和 GitHub Release 的
`latest.json`，确认都返回同一版本。桌面端会按该顺序故障切换；所有安装包仍须通过内置公钥验签。

## 附：环境医生的依赖包镜像（R2 `deps/`，与发版解耦）

环境医生一键安装 PowerShell 7 / Node.js / uv 时，**第一候选源是自家 R2**，公共 GitHub 代理
与官方直连仅作兜底（见 `polaris-kernel/src/doctor/install.rs`）。这条线**平时不用管**，
只在改动那三个版本号时才需要动手：

```powershell
# 版本号在 install.rs 里：PWSH_VER / NODE_VER / UV_VER
# 改任何一个 => 必须把对应架构的包传进 R2，否则 R2 那跳 404，静默退化回不稳定的公共代理
wrangler r2 object put polaris-downloads/deps/<文件名> --file <本地包> --content-type application/x-msi --remote
# key 前缀是 deps/，不含 downloads/（那是 functions/downloads/[[path]].js 的路由前缀）

# 传完必做终验：对着生产域名核字节数 + 文件头魔数（与第 5 步同一套规矩）
# msi 头=d0 cf 11 e0 / zip 头=50 4b 03 04 / tar.gz 头=1f 8b
Invoke-WebRequest "https://llmwiki.cloud/downloads/deps/<文件名>" -Method Head -UseBasicParsing
```

当前在册（x64 + arm64 双架构）：`PowerShell-7.4.6-win-{x64,arm64}.msi`、
`node-v20.18.1-{x64,arm64}.msi`、`uv-{x86_64,aarch64}-pc-windows-msvc.zip`、
`uv-{x86_64,aarch64}-apple-darwin.tar.gz`。32 位 x86 故意没镜像——R2 那跳 404 后自动落到
公共代理，不影响可用性。

> 为什么要自建这一层：公共 GitHub 代理时好时坏（实测 gh-proxy.com 已 500，故已从候选里摘掉），
> 而 PowerShell 的 MSI 有 107MB，慢一点就摸到超时上限 → 「有些概率装不上」。走 R2 后实测
> 8 秒下完（5.3 MB/s），且 R2 出站免费。

### 随安装包内置的运行时（构建期抓取，不入库）

自 2026-07-23 起，**uv / Python / Git Bash 直接打进安装包**——用户不必再装 uv，
Windows 上也不必再装 PowerShell 7（Claude 的 shell 由内置 Git Bash 提供）。
`tauri build` 前由 `beforeBuildCommand` 自动跑 `scripts/fetch-runtimes.mjs` 抓到
`src-tauri/runtime/`（已 gitignore，约 225MB 未压缩 / 3.6k 文件，NSIS lzma 后安装包约 +90MB）。

- 版本锁在 `scripts/fetch-runtimes.mjs` 顶部：`UV_VER`（**与 install.rs 的 UV_VER 保持一致**，
  共用 R2 里同一份包）、`PY_VER`/`PY_TAG`、`GIT_VER`/`GIT_TAG`。
- 下载源顺序同上：R2 → ghfast.top → ghproxy.net → GitHub 直连。**MinGit 与 Python 目前
  还没传 R2**，故走的是公共代理；要提速就按上面的 `wrangler r2 object put` 把
  `MinGit-<ver>-64-bit.zip` 与 `cpython-<ver>+<tag>-<triple>-install_only_stripped.tar.gz`
  传进 `deps/`。
- 抓完可跑真机探针自证：`cargo run -p polaris-kernel --example bundled_runtime_probe`
  （会验证内置 bash 裸跑可用、13 个 unix 工具不落到 System32、bash 里能跑 uv/python）。

## 注意

- **macOS 未签名**：Tauri updater 的 minisign 签名校验与 Apple 公证是两回事。更新包能下载
  并校验通过，但未做 Apple 签名时自替换偶有不稳，且首启仍需 `xattr -dr com.apple.quarantine`。
  要彻底顺滑需 Apple Developer 证书（见 `docs/macos.md`）。
- **更新私钥**：CI 用仓库 secret `TAURI_SIGNING_PRIVATE_KEY` / `..._PASSWORD` 签名；本地构建
  才需显式传私钥文件（密码见项目记忆）。公钥已在 `tauri.conf.json > plugins.updater.pubkey`。
- `mac-build.yml`（`mac-v*` 标签）只出**未签名、无更新能力**的 dmg 供快速分发，**不能自动更新**；
  要自动更新一律走 `release.yml`（`v*` 标签）这条线。
