# P7a 执行报告

2026-10-15。**部分脚本实现，BLOCKED；没有真实端到端通过结论。**
本报告仅为 Codex 执行记录，Claude 重跑、Pi 独立审查后决定模块是否完成。

## 自有文件与行为

| 文件 | 行为 |
| --- | --- |
| `drills/run.sh`、`drills/README.md` | 统一入口、参数、JSONL/退出码约定、每项前置/命令/预期/清理、真实执行边界 |
| `drills/d1.mjs` | 生产构建、HTTP/WS 对话/编码/图片/压缩/取消/切模型及指标；steer/followup 明确 blocked |
| `drills/d2.mjs` | 本次 agentd PID 身份核对后强杀、error→done、重启会话恢复、熔断、真实锁进程驱动 |
| `drills/d3.mjs` | 权限 API、精确路径穿越、Docker fixture inspect/可见性/只读/退出码、server 缺执行器验证及产品链阻塞 |
| `drills/d4.mjs` | 权威本机 HTTP 会话策略、R11、429、备份恢复驱动；跨主机流程仍 blocked |
| `drills/d6.mjs` | 全部 crate 与配置套件逐轮记账、通过率、三个 build 警告计数、物理行数检查 |
| `drills/lib/harness.mjs` | 临时目录、白名单子进程环境、谓词等待、结果/日志、退出清理、秘密脱敏 |
| `drills/lib/process.mjs`、`process-tree.mjs` | 进程启动/日志收集/命令超时/子树停止；按 PID 起始时间防止误杀复用 PID |
| `drills/lib/api.mjs`、`permissions.mjs` | invoke/WS reqId 关联、轮次指标、当前请求授权回应；拒绝串线 |
| `drills/lib/agent.mjs`、`lock-child.mjs` | 生产包及壳构建、只复制 staging 凭据、临时 provider store、session 映射/持久数据；复用 agentd 锁实现 |
| `drills/lib/docker.mjs`、`d4-topology.mjs` | 本地随机 Docker 资源、Compose 角色、临时 CA、公钥指纹、局部 HTTP 账号入口 |
| `drills/lib/fixture.py` | 限定临时路径的账号 fixture、部署 snapshot.py/age 加密恢复适配，无真实用户库探测 |
| `drills/lib/regression.mjs`、`settings.mjs` | 配置类型/边界校验、crate 枚举、警告/行数/汇总函数 |
| `drills/config/runtime.json`、`regression.json` | 默认策略参数与测试命令清单；支持显式文件切换 |
| `drills/test/harness.test.mjs`、`test_fixture.py` | 假进程/事件/API/inspect/配置/清理行为；真实 SQLite/tar 配合假加密的备份恢复断言 |
| `docs/drills/P7a-blockers.md`、本文件、`docs/feature-tree.md`、`docs/feature-tree-p2-protocol.md` | 入口、依赖、默认值/切换、无产品迁移说明、覆盖与未完成范围 |

只修改 drills/ 与 docs/；未改产品代码、未新增依赖、未改 .gitignore、未 commit/push。
工作区另有主控/其它任务的改动（包括 R13 部署/邮件相关文件），未覆盖或回退。

## 已运行的验证

以下命令均在本沙箱执行，未启动真实模型/服务端/Docker：

| 命令 | 真实结果 |
| --- | --- |
| `bash -n drills/run.sh` | 退出 0 |
| `for file in drills/*.mjs drills/lib/*.mjs drills/test/*.mjs; do node --check "$file" || exit; done` | 退出 0；仅语法检查 |
| `node drills/test/harness.test.mjs` | 最近一轮 15/15 pass，0 fail/skip；直接 node:test 文件，无派生 Node 子进程 |
| `python3 -B -m unittest discover -s drills/test -p 'test_*.py'` | 4/4 pass，退出 0；加密函数使用替身，不是 age 实测 |
| Python AST + JSON parse + 自有文件 500 行/500 字符检查（见下方脚本） | 退出 0；27 个自有/维护文件，最大 476 行、445 字符/行 |
| `git diff --check -- drills docs/feature-tree.md docs/feature-tree-p2-protocol.md docs/drills` | 退出 0；只检查已有 tracked diff 的空白问题 |

首次文件尺寸检查发现 feature-tree 原有 500 行，追加后为 510 行；随后按职责拆出 P2.5 协议详表，保留原标题及链接，重新检查。
PNG fixture 初次静态核对发现 IDAT CRC 错误，已修正并加入 Python 第 4 个行为用例（块 CRC 与 zlib 数据验证）。
首次 Node 假命令测试为 13/13；配置和进程身份测试加入后为 15/15。未为了“连续全绿”反复运行产品测试。
最终静态检查使用的内联 Python：

```python
from pathlib import Path
import ast, json
paths = sorted(p for p in Path('drills').rglob('*') if p.is_file())
paths += [Path('docs/feature-tree.md'), Path('docs/feature-tree-p2-protocol.md'), *sorted(Path('docs/drills').glob('P7a-*.md'))]
for path in paths:
    text = path.read_text()
    if path.suffix == '.py': ast.parse(text, filename=str(path))
    if path.suffix == '.json': json.loads(text)
    lines = text.splitlines()
    assert len(lines) <= 500, path
    assert all(len(line) <= 500 for line in lines), path
print('PASS', len(paths), 'files')
```

未运行 `npx tsc --noEmit`：本执行器无新增 TS。没有运行 cargo test/build、npm 产品测试/build、生产打包、Docker build/up/inspect、真实进程强杀、模型调用、openssl/age 或邮件网络接口。

## 未完成与阻塞

详见 [阻塞及覆盖表](P7a-blockers.md)。主要限制：

- D1 steer/followUp 缺服务端分发；当前仅同提供商模型切换，跨提供商和旧 CLI 对照未写。
- D3 server 明确没有 SandboxExecutor，无法按要求证明产品 Docker 停机无回落和受保护项目链接校验。
- D4 自签 CA 未找到可注入 ureq 的产品配置；成员准入/主机入网、relay 生效、跨主机立即读写/吊销传播、离线与清单篡改回滚的完整脚本仍未实现。
- D4 已编写的通知/审批/吊销是权威本机 HTTP session device 路径，不是目标多主机全链路。拓扑 fixture 关闭 relay QUIC 地址发现。
- P7 总清单 D2 的断网/模型 401/429 与提供商故障切换未写；D5 按用户决定推迟。
- 真实运行可能揭示接口、权限、OAuth 刷新、模型输出、构建环境或现有产品测试失败；本次只提供真实可判定的脚本断言，不预测通过。

主控入口示例：`bash drills/run.sh D2 --only lock --keep > /tmp/p7-lock.jsonl`。
完整环境、凭据 staging、全部步骤命令和清理方法见 [README](../../drills/README.md)。
