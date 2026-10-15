# P7a 演练执行器

状态：脚本部分实现，**BLOCKED，未做真实验收**。由 Claude 在沙箱外运行，Pi 独立审查。
已知缺口见 [阻塞与覆盖表](../docs/drills/P7a-blockers.md)；D4 的局部策略演练不能证明跨主机互联完成。
D5 Windows/macOS 实机本轮按用户决定跳过。

## 一条命令与结果

从仓库根目录运行（Linux / WSL2）：

```bash
bash drills/run.sh D1 --keep > /tmp/p7-d1.jsonl
bash drills/run.sh D2 --only lock --keep > /tmp/p7-lock.jsonl
bash drills/run.sh D3 --only docker --keep > /tmp/p7-docker.jsonl
bash drills/run.sh D4 --keep > /tmp/p7-d4.jsonl
bash drills/run.sh D6 --repeat 5 --keep > /tmp/p7-d6.jsonl
```

每个 `d*.mjs` 也可直接 `node drills/d1.mjs --only chat --keep`。`--only` 只选择一个表中步骤，自动执行它的必要前置；未知步骤/参数拒绝。
`--repeat N` 默认 5，整数 1–100，仅 D6 使用。所有等待依据 HTTP readiness、WS open、相关请求事件、进程退出或状态谓词；轮询有截止时间。

stdout 每行一个 JSON，命令日志不混入 stdout：

```json
{"drill":"D1","step":"chat","status":"pass","evidence":{"firstTokenMs":814,"turnMs":1200,"agentdRssKiB":160000},"durationMs":1300}
```

上例仅格式示例，不是真实测量。退出码：0=所选步骤全 pass；1=至少一个 fail；2=有 blocked 且没有 fail。
D6 另输出每轮每套结果、`pass-rate`、三种 `warning-count/*`、`line-count`。配置/前置缺失是 blocked；命令启动后的非零退出或断言不符是 fail。
`evidence.scope` 限定局部检查的证明范围；有 blocked 时不允许把局部 pass 汇总为整个演练通过。

## 前置与数据隔离

- Node >=22.19、Python 3、Bash；已安装仓库根和 agentd 的 lockfile 对应依赖，不新增 npm 包。
- Rust stable、项目构建所需系统库、Cargo 缓存；agentd 打生产包内部会执行 npm ci，真实环境须能访问依赖源。
- D1/D2/D3 模型步骤需要已登录 Pi 的 **staging 副本**。主控先在沙箱外把账号目录复制至例如 `/tmp/p7-pi-input`，再设置 `DRILL_PI_AGENT_DIR=/tmp/p7-pi-input`。
- 执行器不读取 Windows 用户目录或真实 `~/Polaris`；明确拒绝 `/mnt/*/Users/*`、当前 HOME、HOME/Polaris 作为凭据源。输入目录不能含符号链接或设备文件。不会输出 auth.json 内容。
- 每次创建 `/tmp/polaris-drill-*`，子进程 HOME/XDG/TMPDIR 指向本次目录；清除继承的 POLARIS 配置、模型 key、NODE_OPTIONS。仅构建时显式保留 Cargo/Rustup 工具缓存位置。
- 凭据先复制整个 staging agent 目录，再只从复制的 auth.json 导入临时 providers.json；支持 Pi `oauth` / `api_key`，不修改源目录。模型请求可能刷新临时 OAuth 凭据。
- `DRILL_PROVIDER` 默认 `openai-codex`，也接受 `anthropic`。`DRILL_MODEL` 默认不设，沿用真实提供商目录选择；压缩和切换步骤必须显式给模型，不猜测账号支持哪些模型。
- D3 Docker、D4 需要本机 Docker Engine + Compose v2；D4 另需 openssl、age、age-keygen。只操作带本次随机名称的资源，不停止共享 Docker daemon。
- D6 需要前端 dist 已构建、桌面系统库就绪。按 crate 独立测试，避免桌面/server 特性合并；显式 `POLARIS_NET_TEST=1` 启用现有网络测试门控，不排除已知失败项。

凭据切换示例：

```bash
DRILL_PI_AGENT_DIR=/tmp/p7-pi-input DRILL_PROVIDER=anthropic \
DRILL_MODEL='<账号可用模型 ID>' DRILL_MODEL_NEXT='<另一模型 ID>' \
bash drills/run.sh D1 --keep > /tmp/p7-d1.jsonl
```

## D1 Agent 主链路

| step / 命令后缀 | 前置 | 断言 / 预期结果 |
| --- | --- | --- |
| `--only build` | Node/npm/Rust，依赖源 | pack-agentd.mjs 输出到本次临时 runtime；frontend build；release server,collab-net 构建成功 |
| `--only chat` | 模型凭据 | HTTP chat_send → WS reqId 对应 nonce 文本 → done，无 error |
| `--only coding` | 模型能使用 Python/bash 工具 | auto-current 创建 calc.py、执行得到 42、edit 后执行得到 43、read 两个结果；核对四种工具事件与文件 |
| `--only image` | 模型支持图片 | chat_attach_image 上传生成的 PNG fixture，imagePaths 传入；有回答、会话持久文件含 image/png |
| `--only compact` | 显式 DRILL_MODEL | 小窗口、多轮随机惰性文本；必须见持久会话 compaction 条目，且能回忆随机 marker；预算耗尽记 fail |
| `--only abort` | 模型 bash 工具 | 工具事件出现后 chat_cancel；done 后 chat_is_running=false；记录取消耗时 |
| `--only steer` / `followup` | 缺服务端控制入口 | blocked，不绕过 server 直连 daemon 冒充链路通过 |
| `--only switch` | 两个不同且可用的模型 ID | 空闲会话切模型后再发消息；daemon 会话 id 不变、model 改变、随机历史 marker 保留 |

编码/取消时只回应当前 reqId 下的 bash 权限；不会自动批准其它会话。
首字指标从 HTTP 发起到首个非空 delta（含宿主批处理延迟）；单轮到 done；内存为 Linux `/proc` 的 agentd RSS 采样，不是峰值。
图片步骤验证附件运输与有回答，未自动评判视觉语义正确率。切换覆盖同一提供商的模型切换，跨提供商账号切换未实现。
旧 claude-cli 对照未实现，不产生虚构对照数据。

## D2 故障恢复

| step | 前置 | 断言 / 预期结果 |
| --- | --- | --- |
| `recovery` | 同 D1、Linux /proc | 先记随机 marker，真实长回复首个 delta 后 SIGKILL agentd；error 在 done 前；新 PID healthy 且 restarts 增长；原 daemon 会话 id 与历史恢复 |
| `circuit` | 同 D1 | 按配置阈值连续杀本次 agentd；circuitOpen=true、healthy=false、pid=null、lastError 含 CircuitOpen |
| `lock` | 已安装 agentd 的 tsx | 真实持锁进程发 HELD；第二进程先证明 CredentialStoreBusy 再发 CONTENDING；强杀持锁者后第二进程回收并落盘 |

PID 同时核对 PPid=本次 server 与 cmdline=本次生产入口；不按进程名批量 kill。
网络断开、401/429 和提供商故障切换来自 P7 总清单，本 brief 未实现，见阻塞表。

## D3 权限与沙箱

| step | 前置 | 断言 / 预期结果 |
| --- | --- | --- |
| `manual-allow` / `manual-deny` / `manual-timeout` | 同 D1 | 通过 chat_permission_respond 回应或不回应；匹配 responded/expired，核对文件是否生成 |
| `deny` | 同 D1 | 只读 profile 不暴露 write，文件不存在 |
| `auto-current` / `auto-all` | 同 D1 | 项目内 / 临时项目外写文件成功，且无需确认；auto-all 只写本次临时 HOME |
| `traversal` | 同 D1 | symlink + ..、../outside 路径必须实际进入 permission.request；API 拒绝后有 error，项目外文件不存在 |
| `docker` | Docker、镜像构建依赖源 | 从产品 sandbox Dockerfile 构建；P3b 两个 bind mount、KB RO、ALL cap-drop、no-new-privileges、资源/namespace 限制；真实容器检查可见性、RW/RO、exit 37 |
| `server-unavailable` | 同 D1 | useSandbox=true 必须产生 SANDBOX_UNAVAILABLE 证据且不产生宿主 fallback 文件 |
| `docker-down` / `protected-project` | 缺 server SandboxExecutor/Policy | blocked：当前 server 无执行器，不能验证停 Docker 后的执行器行为或针对受保护项目链接的挂载校验 |

auto-current 的越界写语义是“要求批准”，不是无条件硬拒；本演练通过 API 拒绝。
`docker` 是真实容器的策略 fixture，**不是产品创建容器或产品路由执行的证明**。
容器只挂临时项目和 KB；假 sentinel.key 放在临时 HOME/Polaris/data，项目挂载根为 fixture 调整可写权限以适配镜像 uid=1000。

## D4 本机容器与互联

基于新加坡部署的角色划分，生成 Compose JSON：本地构建 server、relay 镜像，启动 authority、relay、member-a、member-b、new-device，加一个现有部署使用的 Caddy TLS 入口。
新随机项目名、loopback 随机端口、独立临时 HOME；openssl 生成一天有效 CA 和 SAN 证书，不请求公网 ACME。
本地 TLS 入口转发 relay HTTP，关闭 QUIC 地址发现；此拓扑不能证明新加坡 UDP/QUIC 传输等价性。

| step | 预期结果与限制 |
| --- | --- |
| `topology` | 5 个目标角色和 TLS 入口启动，HTTP ready，显式 CA 的 curl 访问 authority 和 relay；从公钥重算 SHA-256 指纹 |
| `login` / `relay` / `access` | blocked：成员 ureq 自签 CA 信任入口尚未找到；脚本未实现成员本机准入/真实主机入网和跨主机数据面 |
| `notifications` | **权威本机 HTTP 会话设备**：旧会话收到新设备通知 |
| `approvals` | 同上：新会话高风险操作返回 approval_pending，自批失败，旧会话批准后操作成功 |
| `revoke` | 同上：一键吊销，新会话在配置拉取周期预算内被拒；不是远端传播证明 |
| `offline` / `revocations` | blocked：委托 TLS 链未建立，未实现离线保持会话及清单篡改/回滚投递演练 |
| `r11` | 真实 HTTP 对注册/未注册邮箱比较状态与 JSON；collab signup/reset/login 及 account send_code；两边同时错误不能算 pass |
| `rate` | 真正重复匿名登录请求，必须获得 429，预算来自 runtime.json |
| `backup` | 停本次 authority，复用部署 snapshot.py，用真实 age 加密/解密；校验 tar/SQLite/key 后覆盖恢复，重启校验完整指纹和原账号登录 |

用现有 seed_authority_account example 初始化演练账号，再只在临时 SQLite 提升 owner，并准备互不触发重复发码间隔的邮箱 fixture。
SMTP 指向容器自身 127.0.0.1:1，凭据为假值，不给任何人发信；只验证匿名响应契约，不验证邮件送达。R11 若现有接口同步暴露 SMTP 错误，应报 fail。
backup 复用实际快照/加密实现，外层 Compose 启停由本脚本负责；没有声称完整运行 deploy 的生产 backup.sh/restore.sh。

## D6 回归总表

```bash
bash drills/run.sh D6 --only suites --repeat 5 --keep > /tmp/p7-suites.jsonl
bash drills/run.sh D6 --only builds --keep > /tmp/p7-builds.jsonl
bash drills/run.sh D6 --only lines --keep > /tmp/p7-lines.jsonl
```

- `suites`：枚举 src-tauri/crates 所有 crate 单独 cargo test；collab 带 collab-host,collab-net；另跑两壳 lib/tests、agentd、Vitest、runtime、部署 Python、Docker updater、boundary 和本执行器测试。每轮每套独立记账，失败不会终止剩余轮次。
- `builds`：桌面 Rust build、server,collab-net Rust build、frontend npm build；实际 build，不用 check 替代。每个报告退出码和 warning 数，排除 Rust “generated N warnings” 汇总重复计数。不是 D5 安装包构建。
- `lines`：默认检查配置列出的生产/测试/脚本树全部文本文件，含既有超长文件；不为了通过去改范围外文件。超过 500 物理行或 500 Unicode 字符/行即 fail，stdout 列出违规文件。依赖/构建输出目录不计。
- 内建 ignored 测试计数会报告；未开启的额外人工演练、外部服务评测不由 suite 退出码证明。调试用 scripts/ws-test.mjs 等手工探针不是单元套件，不会指向真实用户服务运行。

## 配置切换

| 配置 | 默认 / 校验 |
| --- | --- |
| `DRILL_TIMEOUT_MS` | 180000，整数 1–3600000，事件/ready 等待预算 |
| `DRILL_COMMAND_TIMEOUT_MS` | 1800000，整数 1000–86400000，单条构建/测试命令预算 |
| `DRILL_PERMISSION_TIMEOUT_MS` | 3000，整数 1–3600000，agentd 手动批准期限 |
| `DRILL_CONTEXT_WINDOW` | 32768，整数 20000–1000000 |
| `DRILL_COMPACT_TURNS` | 12，整数 1–100；压缩失败不自动扩大模型调用预算 |
| `DRILL_MODEL` / `DRILL_MODEL_NEXT` | 默认未设；前者非空且 <=200 字符，后者需不同有效 ID；模型可用性由产品验证 |
| `DRILL_SETTINGS` | 默认 drills/config/runtime.json；必须完整 JSON，未知组/键、类型、范围、重启边界、浮动镜像 tag 拒绝 |
| `DRILL_REGRESSION_CONFIG` | 默认 drills/config/regression.json；argv 数组、仓库内 cwd、唯一 id、恰好三个 builds、行数/列数上限 <=500 |

```bash
cp drills/config/runtime.json /tmp/p7-runtime.json
# 编辑 /tmp/p7-runtime.json，例如 host.failureThreshold=3
DRILL_SETTINGS=/tmp/p7-runtime.json bash drills/run.sh D2 --only circuit --keep
DRILL_CONTEXT_WINDOW=65536 DRILL_COMPACT_TURNS=16 bash drills/run.sh D1 --only compact --keep
cp drills/config/regression.json /tmp/p7-regression.json
# 可调整命令；覆盖范围已变化时不得把定制结果当作默认全量结果
DRILL_REGRESSION_CONFIG=/tmp/p7-regression.json bash drills/run.sh D6 --repeat 2 --keep
```

无产品配置或数据库迁移。取消环境变量即恢复本执行器默认值；所有产品配置都是运行时生成到临时目录。

## 清理与排查

每个演练共用清理：停止自己启动的进程；D3 删除自己创建的容器/镜像，D4 Compose down --volumes 后删除自己的镜像 tag；成功和失败均删除本次 HOME/secrets（含凭据、会话、数据库、CA 私钥和备份）。
`--keep` 仅在 fail/blocked 时保留脱敏 logs、结果与配置，末尾 artifacts 行给 root；**不会保留凭据或继续运行进程/容器**。日志按步名存文件，WS 只记录事件类型/指标，不记录模型正文。
未用 `--keep` 时也应通过 stdout 重定向留存结果。SIGINT/SIGTERM/SIGHUP 会清理后退出；SIGKILL/断电无法触发清理。

硬中断后仅清理已确认属于本次演练的随机资源：从保留的 compose.json 使用原 `p7-…` project 执行 down --volumes，检查 `docker ps -a` 标签和本次镜像 tag，再删除对应临时目录。
不要通配删除其他 `/tmp` 目录或停止宿主 Docker 服务。输入 staging `/tmp/p7-pi-input` 属于主控创建的源副本，执行器不删除；主控用毕自行删除。

## 沙箱内验证

```bash
bash -n drills/run.sh
node drills/test/harness.test.mjs
python3 -B -m unittest discover -s drills/test -p 'test_*.py'
```

Node 测试直接运行单个 node:test 文件，spawn 用事件替身，无真实 Node 子进程；Python 使用真实 SQLite/tar 与假加密函数，绝不将此结果描述为 age 已实跑。
JS 的 node --check 和 Python AST 检查结果见 [执行报告](../docs/drills/P7a-report.md)。
