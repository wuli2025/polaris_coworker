# 审查 brief：P5a 随包运行时、环境诊断改为 Pi、Docker 镜像（含收尾）

三个文件：`P5a-1-diff.patch`（下载/打包脚本、agentd 配置、Docker、后端默认路径与 Windows shell 注入）、`P5a-2-diff.patch`（doctor 新逻辑与对应测试、宿主装配；doctor 超长文件拆分出的纯搬移文件已排除——主控核对原 doctor 163 个函数全部保留，新增 25 个）、`P5a-3-tests.patch`（运行时脚本、agentd、后端相关测试）。**本次附带测试**。

## 验收条件
1. 桌面包随带 Node（版本与 agentd engines 一致，官方 SHASUMS256 校验，幂等）与 agentd 生产包；默认 node/entry 未配置时解析随包路径，开发态回落仓库 `agentd/`。
2. Windows：Pi bash 工具用随包 MinGit bash（`shellPath`），找不到时按 Pi 文档顺序回落并在 doctor 报告。
3. doctor：默认检测 Pi 运行时（Node、agentd 入口、ping 握手、凭据就绪只报是否配置）；Pi 后端下 autopilot 不自动安装 Claude；Claude 安装/校验仅在旧后端显式执行。
4. Docker：默认镜像装 Node + agentd，不强制 Claude；保留旧凭据卷，不删用户数据；健康报告为 Agent 运行时。
5. 下载源：nodejs.org 资源不再拼 GitHub 代理（实测 403）。

## Claude 实测（沙箱外）
真实运行 `node scripts/fetch-runtimes.mjs`（Linux）：Node 22.19.0 从官方源下载 52.4MB、SHASUMS256 校验、解压后 `node -v` = v22.19.0。两壳 `cargo check` 警告 **0**（基线 7，未加 `#[allow]`）；`cargo test -p polaris-kernel --lib --tests` 连续 2 次 **195 通过 0 失败**（原 4 个 Windows 路径基线失败已修复）；agentd 117/117；vitest 32/32；`node --test scripts/runtime/*.test.mjs` 7/7；改动文件 ≤500 行。未实跑：Windows/macOS 打包、Docker 构建。

## 请重点审查
下载与校验能否被绕过（校验文件与安装包取自不同源、HTTP 重定向、解压路径穿越/符号链接）；随包路径解析在安装目录含空格/中文/只读时的行为；Windows shellPath 回落是否可能误用 System32 同名程序；doctor 是否仍会在任何路径下触发 Claude 自动安装；Docker 镜像是否仍携带或打印凭据；agentd 打包是否带入开发依赖或测试文件。
