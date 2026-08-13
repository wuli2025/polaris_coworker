/**
 * ui-smoke-login.mjs —— 统一账号 UI 的真窗口冒烟。
 *
 * 为什么要它:前面所有验收都是后端口径(单测 + HTTP 探针)。前端改了三处登录入口、
 * 删了两百多行旧表单,「编译过」不等于「页面还画得出来」—— 一个 template 里漏改的
 * 变量名,vue-tsc 不一定报,进了页面就是白屏 + 错误边界。
 *
 * 两条踩过的坑照 memory 办:
 *  · 用无头 Chrome,不开真窗口 —— 用户的 app 在跑,single-instance 会把新窗口顶掉;
 *  · **脚本自带 http server 托 dist/**,不用 `npm run dev` 后台起 —— PS 后台起的
 *    dev server 会被回收,于是拿到一堆「404 + 错误边界」的假象,白查半天。
 *
 * 用法:node scripts/ui-smoke-login.mjs   (先 npm run build)
 */
import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { spawn } from "node:child_process";

const ROOT = new URL("../dist/", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
};

// ── 无头 Chrome:跑一段页面内脚本,把断言结果打回来 ──
const CHROME =
  process.env.CHROME_PATH ||
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";

const PROBE = `
  (async () => {
    const out = [];
    const need = (what, ok, detail) => out.push({ what, ok: !!ok, detail: detail ?? "" });
    const $ = (sel) => document.querySelector(sel);
    const text = () => document.body.innerText || "";
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

    await sleep(2500); // 等首屏挂载(splash + 路由)

    need("① 应用挂载出来了(不是白屏)", document.querySelectorAll("*").length > 60,
         document.querySelectorAll("*").length);
    need("② 没有掉进错误边界", !text().includes("出错了") && !text().includes("错误边界"),
         text().slice(0, 200));

    // 进互联页
    const nav = [...document.querySelectorAll("button,a,[role=button]")]
      .find((e) => (e.innerText || "").trim().startsWith("互联"));
    need("③ 找得到「互联」入口", !!nav, "");
    if (nav) { nav.click(); await sleep(1200); }

    const t = text();
    need("④ 互联页画出来了", t.length > 50, t.slice(0, 120));
    need("⑤ 主路径已是邮箱登录", t.includes("邮箱"), t.slice(0, 400));
    need("⑥ 首屏不再要求填账号密码", !t.includes("账号(用户名或邮箱)"), "");
    need("⑦ 首屏不再有「入网」表单按钮", !t.includes("入网并自动连上我的设备"), "");
    need("⑧ 首屏不再教「把这台电脑设为主机」", !t.includes("把这台电脑设为主机"), "");
    need("⑨ 两步接入文案已上", t.includes("一个邮箱") || t.includes("同一个邮箱"), "");

    // 登录组件本体去「协作」页验:互联页在浏览器形态下视作「已在跟一台主机对话」,
    // 不出登录框(那是 server 版的既有行为,不是这次改动引入的)。
    const nav2 = [...document.querySelectorAll("button,a,[role=button]")]
      .find((e) => (e.innerText || "").trim().startsWith("协作"));
    need("⑩ 找得到「协作」入口", !!nav2, "");
    if (nav2) { nav2.click(); await sleep(1500); }
    const t2 = text();
    need("⑪ 协作页主路径也是邮箱验证码", t2.includes("用邮箱登录"), t2.slice(0, 300));
    need("⑫ 旧的用户名密码表单已收进折叠", t2.includes("登不上?用旧方式"), "");

    // 登录表单可交互(输入框 + 发码按钮)
    const emailInput = [...document.querySelectorAll("input")]
      .find((i) => (i.placeholder || "").includes("邮箱"));
    need("⑬ 邮箱输入框在", !!emailInput, "");
    const sendBtn = [...document.querySelectorAll("button")]
      .find((b) => (b.innerText || "").includes("发送验证码"));
    need("⑭ 发码按钮在", !!sendBtn, "");
    if (emailInput && sendBtn) {
      need("⑮ 空邮箱时发码按钮是禁用的(不让白跑一趟)", sendBtn.disabled, "");
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
      setter.call(emailInput, "smoke@example.com");
      emailInput.dispatchEvent(new Event("input", { bubbles: true }));
      await sleep(300);
      need("⑯ 填了合法邮箱后按钮解禁", !sendBtn.disabled, "");
    }
    // 结果塞进 DOM —— --dump-dom 拿得到,比抓 console 稳(headless 的 console
    // 要额外开 --enable-logging,还会被一堆无关日志淹掉)。
    const box = document.createElement("div");
    box.id = "polaris-smoke";
    box.textContent = "POLARIS_SMOKE " + JSON.stringify(out);
    document.documentElement.appendChild(box);
  })();
`;

const server = createServer(async (req, res) => {
  try {
    const url = decodeURIComponent((req.url || "/").split("?")[0]);
    let file = normalize(join(ROOT, url === "/" ? "index.html" : url));
    if (!file.startsWith(normalize(ROOT))) {
      res.writeHead(403).end("forbidden");
      return;
    }
    let s = await stat(file).catch(() => null);
    // SPA 兜底:非资源路径一律回 index.html
    if (!s || s.isDirectory()) {
      file = join(ROOT, "index.html");
      s = await stat(file).catch(() => null);
      if (!s) {
        res.writeHead(404).end("dist/ 不存在 —— 先跑 npm run build");
        return;
      }
    }
    let body = await readFile(file);
    // 探针由**服务器注进 index.html**。`--evaluate-on-new-document` 是 CDP 的方法
    // 不是 Chrome 的命令行开关(传进去只会被当成未知参数吞掉,于是拿不到任何结果 ——
    // 这一步卡过一次)。注进 HTML 就没有这层不确定性。
    if (file.endsWith("index.html")) {
      body = Buffer.from(
        body.toString("utf8").replace("</body>", `<script>${PROBE}</script></body>`),
        "utf8"
      );
    }
    res.writeHead(200, { "content-type": MIME[extname(file)] || "application/octet-stream" });
    res.end(body);
  } catch (e) {
    res.writeHead(500).end(String(e));
  }
});

const port = await new Promise((r) => server.listen(0, "127.0.0.1", () => r(server.address().port)));
const base = `http://127.0.0.1:${port}`;
console.log(`静态服务器 ${base}(托 dist/)`);


const args = [
  "--headless=new",
  "--disable-gpu",
  "--no-sandbox",
  "--window-size=1440,900",
  "--virtual-time-budget=12000",
  "--dump-dom",
  base,
];

const chrome = spawn(CHROME, args, { stdio: ["ignore", "pipe", "pipe"] });
let stdout = "";
let stderr = "";
chrome.stdout.on("data", (d) => (stdout += d));
chrome.stderr.on("data", (d) => (stderr += d));

const code = await new Promise((r) => chrome.on("close", r));
server.close();

// dump 出来的是一整坨 DOM,结果不一定单独占一行 —— 整份里抓那个标记 div。
const all = stderr + stdout;
const m = all.match(/POLARIS_SMOKE (\[.*?\])<\/div>/s);
if (!m) {
  console.error("没拿到断言结果(chrome 退出码 " + code + ")");
  console.error(all.slice(-3000));
  process.exit(1);
}
const results = JSON.parse(m[1].replace(/&quot;/g, '"').replace(/&amp;/g, "&"));
let bad = 0;
for (const r of results) {
  console.log(`  ${r.ok ? "✓" : "✗"} ${r.what}${r.ok ? "" : " —— " + r.detail}`);
  if (!r.ok) bad++;
}
console.log(bad ? `\n${bad} 条未过` : "\nALL PASS");
process.exit(bad ? 1 : 0);
