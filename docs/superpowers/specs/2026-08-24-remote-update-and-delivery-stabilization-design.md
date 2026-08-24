# Polaris 全路径远程更新与交付稳定化设计

日期：2026-08-24

## 1. 目标

本次工作把已经通过长稳压测的 fast Agent 交付到现有 GitHub PR，并消除阻止安全合并与后续远程更新的已知缺陷。最终交付覆盖四类现有运行入口：

1. Windows 桌面端通过 Tauri updater 获取并安装签名 NSIS 更新包。
2. macOS 桌面端通过 Tauri updater 获取并安装签名 universal app 更新包。
3. Docker 与 NAS 部署通过远程 Web 页面触发隔离 updater，以 OCI build revision 判断更新并确认容器替换。
4. 远程 Web 管理沿用 server 数据面和 owner 权限，在容器重启期间持续恢复更新状态。

“全路径”同时要求下载或镜像源具备有界故障切换：桌面端覆盖 GitHub Releases、GitHub 代理、Cloudflare 自托管和 GitHub 直连；容器端覆盖 GHCR 和显式配置的 HTTPS OCI registry。

## 2. 范围与非目标

### 2.1 本次范围

- 修复 Unix 进程 PATH 写入与查重分隔符不一致的真实缺陷。
- 让 doctor 的 Windows 路径语义测试在 Linux 和 Windows CI 上都确定、有效。
- 基于最新 `origin/main` 重建 Polaris PR，剔除纯 CRLF 改写并解决 Docker update overlay 冲突。
- 保持 i-agent fast 模式、Claude work 模式、provider 隔离、取消、看门狗、最终交付审计和子进程清理能力。
- 加固桌面和容器的远程更新源、状态、权限、失败收口与发布清单校验。
- 更新现有 GitHub PR，不新建重复 PR；Polaris 分支允许使用用户已批准的 `--force-with-lease` 安全重写。

### 2.2 非目标

- 不让裸运行的 Linux/WSL `polaris-server` 二进制覆盖自身、重启系统服务或修改宿主机 service manager。
- 不把 i-agent 源码或二进制复制进 Polaris 安装包；许可证问题解决前继续通过 `POLARIS_FAST_AGENT_BIN` 接入。
- 不把无关的历史 Clippy 风格债混入本次 PR。
- 不自动创建版本 tag、合并 `main` 或发布生产 Release；这些是独立的发布授权动作。
- 不触碰或提交当前工作区已有的 `package-lock.json` 修改。

## 3. 方案选择

### 方案 A：保留双更新引擎，统一状态契约（采用）

桌面端继续使用 Tauri updater 和 minisign；Docker/NAS 继续使用 OCI registry、隔离 Watchtower 与容器 build identity。两条链路共享前端概念和成功语义，但不强行共享高权限实现。

优点是复用现有成熟机制、权限最小、平台行为自然，并能用各自最强的完整性证明：桌面包验签，容器镜像验 digest/revision。

### 方案 B：所有平台统一依赖 updater daemon

该方案需要在桌面系统安装常驻高权限服务，并让它同时理解安装包和容器。部署、升级和攻击面都明显扩大，因此不采用。

### 方案 C：远程脚本或手工命令

该方案无法可靠跨重启确认目标版本，也无法给远程页面提供一致状态，容易把脚本返回成功误判为产品更新成功，因此不采用。

## 4. 总体架构

### 4.1 统一更新状态

前端继续通过 `useUpdater` 暴露统一用户语义：

- `idle`：尚未检查。
- `checking`：正在查询更新目标。
- `up-to-date`：运行版本与目标一致。
- `available`：发现可验证的新目标。
- `downloading` / `triggering`：桌面下载包或容器 updater 已接单。
- `installing` / `waiting-restart`：正在安装或等待容器替换。
- `succeeded`：新进程已经运行目标版本并通过 ready 探测。
- `error` / `unconfirmed`：已明确失败，或在截止时间内无法证明成功。

两条后端链路可以保留内部状态名称，但前端不得把 HTTP 200、下载完成或 Watchtower 返回当作最终成功。

### 4.2 桌面更新链路

1. Tauri updater 从有序 manifest endpoints 检查 `latest.json`。
2. manifest 必须包含当前平台键和 minisign signature。
3. 客户端把 manifest 中的 GitHub asset URL 归一为裸 GitHub URL，再生成去重的候选下载源。
4. 每个候选源都受连接、总时长和停滞看门狗约束；失败后切换下一源。
5. 任一候选下载的字节仍由 Tauri updater 使用内置公钥验签。镜像返回 HTML、截断包或被篡改内容一律失败。
6. 安装后由新进程版本与持久化的待安装版本对照，确认完成并清理标记。

候选顺序以稳定自托管源和可用代理为优先，GitHub 直连永远作为最终兜底。候选生成必须保持确定性、去重且不产生代理套娃。

### 4.3 Docker、NAS 与远程 Web 链路

1. server 查询运行 tag 的 OCI index/config，得到目标 digest、build revision 和版本。
2. 当前容器从镜像内 build marker 读取自身 revision，避免复用旧容器环境变量造成假版本。
3. 只有目标 revision 不同且 updater 能力就绪时，`docker_update` 才创建持久化 request。
4. App 只调用固定 Compose 内网端点的隔离 Watchtower；Docker socket 永不进入 Polaris App 容器。
5. 远程页面拿到 request ID 后，跨旧容器断连继续轮询 `/api/build`、`/api/ready` 和 request 状态。
6. 只有 boot ID 已变化、build revision 等于目标值且 ready 成功，状态才能进入 `succeeded`。
7. Watchtower 失败、目标 build 不匹配或超过截止时间时进入明确终态，并给出可操作的日志与手工恢复指引。

GHCR 使用匿名 pull token 和 OCI digest；自定义 registry 必须同时配置 registry API。生产环境只接受 HTTPS，明文 HTTP 仅允许 loopback/测试 registry 或显式测试开关。

### 4.4 远程权限

- 更新检查属于只读能力，可向已通过基础访问闸的 owner 展示。
- 更新触发与状态文件读取走 server owner-only 数据面。
- 公网远程访问必须通过账号会话或 `POLARIS_AUTH_TOKEN`；无鉴权请求不得获得容器替换能力。
- 家用 NAS 的局域网免口令体验继续受 OriginGate/LAN 边界约束，不为公网开放匿名更新入口。
- Watchtower token 与 Polaris 登录 token 相互独立；任何响应、报告和日志都不得回显 token。

## 5. GitHub 发布清单

正式 `v*` 构建继续产出：

- `windows-x86_64`：NSIS setup executable 与 `.sig`。
- `darwin-x86_64`：universal `Polaris.app.tar.gz` 与 `.sig`。
- `darwin-aarch64`：与 Intel 键指向同一个 universal app tarball 和签名。
- `.dmg`：供 macOS 全新安装，不作为 Tauri 自更新 payload。

发布校验器根据构建产物生成或验证确定性的 `latest.json`，至少检查：

- version 与 tag/应用版本一致；
- 三个平台键完整且没有未知必需键遗漏；
- signature 非空且不被换行或 JSON 转义破坏；
- URL 文件名与实际 artifact 一致；
- URL 使用允许的 HTTPS host，且代理 URL 内只嵌套一层 GitHub URL；
- Cloudflare 资产响应必须符合预期魔数和字节长度，不能只凭 HTTP 200 判断。

由于仓库当前 Actions token 无法创建 Release，工作流的强制职责是构建、签名、生成/校验 manifest 并上传完整 artifact 集。生产 Release 创建仍由 owner 授权执行；本次不伪装成已经发布新版。

## 6. 分支与交付策略

Polaris 当前 feature branch 同 `origin/main` 已分叉，并包含一个 43 文件、约 2.1 万行的纯 CRLF snapshot。直接 merge 会保留噪声并在 `docker-compose.update.yml` 冲突。

采用以下策略：

1. 在隔离 worktree 中从最新 `origin/main` 构造干净集成分支。
2. 只重放设计文档、压测器、fast Agent 集成和确定性修复，跳过纯换行符 commit。
3. `docker-compose.update.yml` 以最新 NAS 安全更新设计为基线，同时保留本次确需的 server/Forge 产物配置。
4. 在完整验证和独立代码审查后，用 `git push --force-with-lease` 更新现有 Polaris PR 分支。
5. i-agent 保持现有线性分支，只追加必要修复并普通 push。
6. 当前工作区的 `package-lock.json` 用户修改始终留在原 worktree，既不暂存也不提交。

`--force-with-lease` 必须先核对远端 head 仍等于本地已知 SHA；若远端被他人更新则停止，不覆盖新提交。

## 7. Doctor 跨平台修复

### 7.1 Unix PATH 真实缺陷

进程 PATH 的解析、查重和拼接必须使用同一平台语义：Windows 使用 `;` 且比较忽略大小写，Unix 使用 `:` 且保持大小写敏感。测试要保存并恢复原 PATH，避免并发测试污染进程全局状态。

### 7.2 Windows 路径测试

Windows 路径不能在 Unix 上直接依赖 `std::path::Path` 的 component/parent 语义。测试分成两类：

- 平台无关行为使用当前平台的合法临时路径。
- Windows 驱动器、反斜杠、npm shim 等专属语义只在 Windows 编译运行，或由接收显式路径风格的纯函数测试。

目标不是用 `#[cfg(windows)]` 隐藏产品缺陷，而是让每个测试验证其运行平台真实会执行的代码路径。

## 8. 错误处理与恢复

- manifest 单源失败不覆盖已经持久化的可用版本；所有源失败才进入 error。
- 桌面下载失败应汇总尝试过的候选类别，但不得包含签名、token 或敏感查询参数。
- 更新操作保持 single-flight；重复点击不能启动第二个安装或容器替换。
- 容器 request 使用严格 ID 校验并限制在固定状态目录，禁止路径逃逸。
- 旧容器断连是预期中间态；只有明确 `failed`/`unconfirmed` 或截止时间到达才停止轮询。
- runner、server 或测试进程退出时继续统一调用子进程与 session pool 清理，更新验证不得留下 i-agent、Polaris server、Forge、浏览器或 Watchtower 测试进程。

## 9. 测试策略

所有行为修改遵循 TDD：先加入能在当前实现上失败的最小测试，确认失败原因正确，再实现单一修复并确认转绿。

### 9.1 单元与组件测试

- PATH separator、大小写、尾斜杠、幂等和环境恢复。
- npm install root 分组和多版本冲突在对应平台上的真实语义。
- desktop manifest/asset 候选展开、去重、代理拆套和非 GitHub URL 保留。
- updater 状态机的持久化、single-flight、错误回退和终态唯一性。
- registry URL、HTTP 安全边界、request ID、revision/digest 与成功判定。
- `useUpdater` 对断连、失败、错误账号、重启、目标 revision 不匹配和最终 ready 的处理。

### 9.2 集成与发布测试

- Polaris unit、stress harness、pipeline 和 server 清理套件。
- i-agent unit、stdio integration、CloakBrowser 本地 HTML/HTTP。
- disposable registry + Watchtower Docker E2E，覆盖成功、拉取失败、无更新和截止时间。
- release manifest 结构、平台键、URL、签名字段和 artifact 文件名校验。
- Windows/macOS GitHub Actions 编译矩阵；无法在本机替代的签名/安装行为由 CI artifact 证明。

### 9.3 代表性回归

修复后重跑 fast Agent 浏览器、代码、PPT、并发、恢复和取消代表矩阵。已完成的 2 小时 37 分长稳是基线证据；除非修改触及 Agent 协议或出现新不稳定，不重复无意义的一小时供应商额度消耗。

## 10. 完成标准

- Polaris full kernel 测试零失败；明确标记的 ignored 测试可保留。
- Polaris unit、stress、pipeline、server cleanup 和生产构建全部通过。
- i-agent unit、stdio integration、Linux/Windows 构建和现有 CI 全部通过。
- desktop 更新源与 release manifest 校验覆盖 Windows 和两种 macOS architecture key。
- Docker/NAS 更新 E2E 能证明目标容器替换；失败场景在有限时间内退出等待态。
- 远程非 owner 或未通过公网鉴权的请求不能触发更新。
- 两个 PR 不包含完整密钥；Windows/WSL 无残留相关进程。
- Polaris PR 可干净合并到最新 `main`，且不再包含 43 文件的纯 CRLF 改写。
- 两个 GitHub PR 已更新到验证过的 commit，并在说明中列出验证证据和仍需 owner 执行的 Release 动作。
