---
id: browser-use
name: 浏览器智能体 browser-use
description: 给一句高层目标（如「在某网站查到 X 并把表单填好提交」「登录后导出近一个月账单」），自动完成多步网页任务。底层浏览器强制走 CloakBrowser 的隐身 Chromium（过 Cloudflare/反爬），绝不用裸 Playwright。MiniMax 由当前 Agent 直接控制 CloakBrowser；其它兼容模型可用 browser-use runner。适合打开网页、抓取、填表、点击、截图和随页面变化决策的网页自动化。
source: third-party
author: Polaris
created_at: 1750000000
---

# 浏览器智能体 browser-use（驱动 CloakBrowser）

当任务是**高层、多步、需要临场决策**的网页自动化——「帮我在 X 网站完成某流程」「自动登录后把近一个月的订单导出来」「在这个站把表单按要求填好并提交」——用 **browser-use 智能体**：你只给目标，它自己观察页面、规划并执行一连串点击/输入/滚动/翻页，直到达成。

## 先选路线（必须）

| 当前对话模型 | 执行路线 |
|---|---|
| MiniMax（模型名含 `MiniMax`，或 `ANTHROPIC_BASE_URL` 含 `minimaxi.com`） | **当前 Agent 直接写并运行 CloakBrowser Python**，自己完成观察→操作→核验；不要启动 browser-use runner |
| 其它已验证兼容 browser-use 工具格式的模型 | 复杂多步任务可用下方 runner；简单任务仍直接用 CloakBrowser |

MiniMax 与 browser-use 0.13.8 的工具调用格式不完全兼容。嵌套 browser-use 会出现 `thinking expected string`、缺少 `action` 或“Expected tool use”错误；这不是重试能修好的瞬时故障，必须走直接路线。

## 铁律：底层必须是 CloakBrowser

browser-use 默认会启自带的浏览器（Patchright）。**本项目禁止**——任何操纵浏览器都必须走 **CloakBrowser** 的源码级隐身 Chromium（过 Cloudflare Turnstile / reCAPTCHA / 指纹检测）。本技能自带的 runner 已经把这件事做好了：用 CloakBrowser 启一个带远程调试端口（CDP）的隐身 Chromium，再把 browser-use 通过 `cdp_url` 接上去。非 MiniMax 路线直接用这个 runner，不要另起 browser-use 的默认浏览器。

以下行为同样禁止：

- 禁止 `find /`、`rg /`、`ls -R /` 或任何从文件系统根目录开始的递归搜索。定位依赖只查当前工作目录、`~/Polaris/skills`、`~/Polaris/plugins`，或用 Python import 自省。
- 禁止导入 `playwright.sync_api` / `playwright.async_api` 来代替 CloakBrowser，也禁止在 runner 报错后生成裸 Playwright 兜底脚本。
- CloakBrowser 或 runner 失败就保留错误并以失败结束；不得换成普通浏览器后声称任务完成。

## 怎么用（一条命令）

脚本在 `~/Polaris/skills/browser-use/scripts/browser_use_runner.py`
（Windows：`%USERPROFILE%\Polaris\skills\browser-use\scripts\browser_use_runner.py`）。

```bash
uv run --no-project ~/Polaris/skills/browser-use/scripts/browser_use_runner.py \
  "用大白话写清楚要达成的目标" \
  --start-url "https://起始页（可选）" \
  --out "<结果输出目录>" \
  --max-steps 25            # 步数上限，防跑飞
  # --headful               # 想看着它操作就加（默认无头）
  # --model "<模型名>"      # 覆盖 LLM 模型（默认读 Polaris 当前供应商）
```

- 优先 `uv run --no-project`（runner 已用 PEP 723 钉死 `browser-use==0.13.8` / `cloakbrowser==0.5.8`，uv 自动建隔离环境并复用缓存；不要另建 pyproject/venv，也不要改 runner）。
- 没有 uv 的环境用 `python3 browser_use_runner.py ...` 也行。

## runner 替你做了什么

1. **确保依赖**：`uv run` 按 runner 顶部 PEP 723 声明自动准备固定版本；仅在没有 uv、直接用 Python 时才需要手工安装对应版本。
2. **起隐身浏览器**：主线程用 CloakBrowser 启一个带 `--remote-debugging-port` 的隐身 Chromium并等 CDP；browser-use 的异步循环在独立工作线程运行。同步 Playwright 与 asyncio 物理隔离，规避 `Sync API inside asyncio loop` 和 `asyncio.run() cannot be called from a running event loop`。
3. **接上智能体**：browser-use 经 `cdp_url` 连到这个端口（兼容新旧版的 `Browser(cdp_url=)` / `BrowserConfig(cdp_url=)` 入参）。
4. **选 LLM**：从进程环境变量取 Polaris 当前供应商凭证（`ANTHROPIC_*` 或 `OPENAI_*`），无需另配 key。可用 `--model` 覆盖。
5. **跑 + 收尾**：执行智能体循环，把最终结论写到 `<out>/browser_use_result.txt` 并打印绝对路径；结束后关闭浏览器。若 browser-use 没有返回最终结论，runner 会保留诊断文件并以非零状态退出，调用方必须把它当失败。

## 版本可能不同——按需自适应

browser-use 的 API 各版本变动较大。runner 是**能跑的脚手架**，已对常见版本做了多路兼容；万一仍因 API 不匹配报错：先 `pip show browser-use` 看版本，再据此微调 runner 里构造 `Browser` / `Agent` / LLM 的那几行。**唯一不可破的不变量：底层浏览器必须是经 CDP 接上的 CloakBrowser，绝不用 browser-use 自带浏览器。**

## 安全约定

- **不可逆动作先确认**：提交订单/付款、发布内容、删除数据、改密码这类动作，先把将要做的事讲清楚、等用户确认，**不要默默替用户拍板**。
- 需要登录的站，引导用户在 `--headful` 下扫码/手动登录一次（CloakBrowser 可持久化会话）。
- 抓取/导出的结果存到工作目录并回报**绝对路径**。
- 反爬/风控场景务必走 CloakBrowser 的 `humanize=True`（runner 已默认开）。

## 不适用

- **简单单步**（只打开一个页面截图、抓一段已知文本）：直接用 **CloakBrowser** 写几行 `page.goto/screenshot` 更快，不必动用智能体（省 token、更确定）。
- **大文件下载**：用「极速下载 TurboDownload」技能。
