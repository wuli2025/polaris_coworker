# 默认浏览器模式 (CloakBrowser)

Polaris 的**默认浏览器**是 **CloakBrowser** —— 一个源码级改造的隐身 Chromium，可直接替换 Playwright / Puppeteer，能绕过 Cloudflare Turnstile、reCAPTCHA、FingerprintJS、BrowserScan 等机器人检测。

凡是需要「打开网页 / 抓取 / 填表 / 点击 / 截图 / 网页自动化」的任务，**优先用 CloakBrowser**，而不是普通 `requests` 或原生 `playwright`。

当前模型是 MiniMax（模型名含 `MiniMax`，或 `ANTHROPIC_BASE_URL` 含 `minimaxi.com`）时，直接由当前 Agent 编写并运行 CloakBrowser 脚本；不要再嵌套 browser-use 0.13.8，它的工具格式与 MiniMax 不完全兼容。

## 启用方式（Python，drop-in 替换）
1. 安装（首次运行会自动下载隐身 Chromium 二进制 ~200MB，本地缓存）：
   - 在线：`pip install cloakbrowser`
   - 离线（Polaris 已附带源码副本）：`pip install ~/Polaris/plugins/cloakbrowser`
2. 用法与 Playwright 完全一致，只换 import：
   ```python
   from cloakbrowser import launch

   browser = launch(humanize=True)      # humanize=True：拟人化鼠标/键盘/滚动
   page = browser.new_page()
   page.goto("https://target.example")
   # ... 抓取 / 点击 / 填表 / page.screenshot(path="shot.png")
   browser.close()
   ```
3. 需要代理 + 指纹地理定位：`launch(proxy="socks5://user:pass@host:port", geoip=True)`
4. 需要持久会话（保留 cookie / localStorage、绕过隐身检测）：`launch_persistent_context(...)`

## 约定
- 默认无头；需要观察过程时 `launch(headless=False)`（参数是 `headless=`，不是 `headed=`）
- 抓取结果存到工作目录并回报绝对路径
- 反爬 / 风控场景务必带 `humanize=True`
- 禁止 `find /`、`rg /`、`ls -R /` 等从根目录递归搜索；只查工作目录、`~/Polaris/skills`、`~/Polaris/plugins` 或用 Python import 自省
- 禁止用原生 `playwright.sync_api` / `playwright.async_api` 兜底；CloakBrowser 失败时应原样报告失败

## 随时移除
本插件可随时拿掉：在「技能中心」把「CloakBrowser 浏览器」开关关掉即可；关闭后需要浏览器的任务应明确报告能力不可用，不得静默回退到裸 Playwright。删除 `~/Polaris/plugins/cloakbrowser` 可一并清除离线源码副本。
