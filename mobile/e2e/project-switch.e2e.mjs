/**
 * 手机端「说一句就进电脑项目」的真浏览器验收。
 *
 * 起一个假主机(静态托管 mobile/dist + /api/invoke 记账),用无头 Chrome 以 390×844
 * 打开真手机壳,在输入框里真打字、真点发送,然后看主机**实际收到了哪些命令** ——
 * 验的就是 send() → tryOpenProject → switchProject → bindConvToProject → chat_send 这条链。
 */
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

// 跑法(先 npm run build 出 dist):  node mobile/e2e/project-switch.e2e.mjs
// 想看长什么样再加 --shots,截图落在 mobile/e2e/shots/。
const HERE = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.resolve(HERE, "..", "dist");
const SHOTS = process.argv.includes("--shots") ? path.join(HERE, "shots") : null;
const PORT = 18099;
const CHROME =
  process.env.POLARIS_CHROME ??
  [
    "C:/Program Files/Google/Chrome/Application/chrome.exe",
    "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/usr/bin/google-chrome",
  ].find((p) => fs.existsSync(p));
const PROFILE = path.join(os.tmpdir(), "polaris-ui-e2e-chrome");
const DBG = 9333;

const calls = []; // 主机收到的每一条 invoke
const MIME = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".svg": "image/svg+xml", ".json": "application/json", ".woff2": "font/woff2" };

// 电脑上的两个项目(带真实感的工作目录)
const PROJECTS = [
  { id: "p-alpha", name: "北极星", work_dir: "D:\\polaris\\polaris-app", created_at: 1, archived: false },
  { id: "p-beta", name: "公司官网", work_dir: "D:\\code\\company-site", created_at: 2, archived: false },
];
const APPS = [{ slug: "notes", name: "笔记", port: 3000, path: "/", note: "" }];
let convSeq = 0;

function handleInvoke(cmd, args) {
  calls.push({ cmd, args });
  switch (cmd) {
    case "conv_list_projects": return PROJECTS;
    case "conv_list_conversations": return [];
    case "conv_get_messages": return [];
    case "conv_create_conversation": {
      const p = PROJECTS.find((x) => x.id === args.projectId);
      if (!p) throw new Error(`project ${args.projectId} 不存在`);
      return { id: `c-host-${++convSeq}`, project_id: p.id, title: "新对话", created_at: Date.now(), updated_at: Date.now(), archived: false };
    }
    case "conv_create_project": {
      const p = { id: `p-new-${PROJECTS.length}`, name: args.name, work_dir: null, created_at: Date.now(), archived: false };
      PROJECTS.push(p);
      return p;
    }
    case "conv_set_project_work_dir": {
      const p = PROJECTS.find((x) => x.id === args.projectId);
      if (!p) throw new Error("项目不存在");
      // 学主机:目录必须真实存在,否则拒绝(手输路径最容易打错)
      if (args.workDir && !fs.existsSync(args.workDir)) throw new Error(`工作目录不存在或不是文件夹: ${args.workDir}`);
      p.work_dir = args.workDir ?? null;
      return null;
    }
    case "chat_send": return "req-1";
    case "chat_cancel": return null;
    case "app_pub_list": return APPS;
    case "app_open": return { sid: "s1", url: "/x/s1/" };
    case "provider_list": return { providers: [], currentId: "" };
    case "list_skills": return [];
    case "sys_stats": return { cpu: 1, mem: 1 };
    case "voice_transcribe_audio": throw new Error("暂不支持");
    default: return {};
  }
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://127.0.0.1:${PORT}`);
  const send = (code, body, type = "application/json") => {
    res.writeHead(code, { "content-type": type, "access-control-allow-origin": "*" });
    res.end(typeof body === "string" ? body : JSON.stringify(body));
  };
  if (url.pathname === "/api/health") return send(200, { ok: true });
  if (url.pathname === "/api/collab/me") return send(200, { username: "test", role: "owner" });
  if (url.pathname === "/api/account/info") return send(200, { mode: "local", emailRequired: false });
  if (url.pathname === "/api/invoke") {
    let raw = "";
    req.on("data", (c) => (raw += c));
    req.on("end", () => {
      try {
        const { cmd, args } = JSON.parse(raw || "{}");
        send(200, handleInvoke(cmd, args ?? {}));
      } catch (e) {
        send(400, { error: String(e.message ?? e) });
      }
    });
    return;
  }
  if (url.pathname.startsWith("/api/")) return send(200, {});
  // 静态:手机壳本体
  const rel = url.pathname === "/" ? "/index.html" : url.pathname;
  const file = path.join(DIST, rel);
  if (!file.startsWith(path.normalize(DIST)) || !fs.existsSync(file)) return send(404, "not found", "text/plain");
  // 静态件必须原样吐字节:走 send() 会被 JSON.stringify 成 Buffer 的 JSON(踩过一次)
  res.writeHead(200, { "content-type": MIME[path.extname(file)] ?? "application/octet-stream" });
  res.end(fs.readFileSync(file));
});
server.on("upgrade", (_r, socket) => socket.destroy()); // /ws:本测不验流式,前端会自行退避重连

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

class CDP {
  constructor(ws) { this.ws = ws; this.id = 0; this.pending = new Map(); }
  static async attach(wsUrl) {
    const ws = new WebSocket(wsUrl);
    await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
    const c = new CDP(ws);
    ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.id && c.pending.has(m.id)) { c.pending.get(m.id)(m); c.pending.delete(m.id); }
    };
    return c;
  }
  send(method, params = {}) {
    const id = ++this.id;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((r) => this.pending.set(id, r));
  }
  async eval(expression) {
    const m = await this.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (m.result?.exceptionDetails) throw new Error("页面报错: " + JSON.stringify(m.result.exceptionDetails.exception?.description ?? m.result.exceptionDetails));
    return m.result?.result?.value;
  }
}

/** 存一张 390×844 的截图(仅 --shots 时)。 */
async function shot(cdp, name) {
  if (!SHOTS) return;
  fs.mkdirSync(SHOTS, { recursive: true });
  const m = await cdp.send("Page.captureScreenshot", { format: "png" });
  const data = m.result?.data;
  if (data) fs.writeFileSync(path.join(SHOTS, `${name}.png`), Buffer.from(data, "base64"));
}

let bad = 0;
function check(label, ok, detail = "") {
  if (ok) console.log(`✓ ${label}`);
  else { bad++; console.log(`✗ ${label}${detail ? "\n    " + detail : ""}`); }
}

/** 在输入框真打字并点发送。 */
async function say(cdp, text) {
  const mark = calls.length;
  const typed = await cdp.eval(`(() => {
    const t = document.querySelector('.dock textarea');
    if (!t) return false;
    const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
    setter.call(t, ${JSON.stringify(text)});
    t.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  })()`);
  if (!typed) throw new Error("没找到输入框(.dock textarea)");
  await sleep(120); // 等 Vue 把发送键从 disabled 放开
  const clicked = await cdp.eval(`(() => {
    const b = document.querySelector('.dock button.go');
    if (!b || b.disabled) return false;
    b.click();
    return true;
  })()`);
  if (!clicked) throw new Error("发送键点不了");
  await sleep(900); // 等这一轮的请求发完
  return calls.slice(mark);
}
const cmds = (list) => list.map((c) => c.cmd);

let chrome;
try {
  await new Promise((r) => server.listen(PORT, "127.0.0.1", r));
  console.log(`假主机 http://127.0.0.1:${PORT}\n`);

  fs.rmSync(PROFILE, { recursive: true, force: true });
  chrome = spawn(CHROME, [
    "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
    `--user-data-dir=${PROFILE}`, `--remote-debugging-port=${DBG}`,
    "--window-size=390,844", `http://127.0.0.1:${PORT}/`,
  ], { stdio: "ignore" });

  // 等 CDP 就绪并找到页面 target
  let target = null;
  for (let i = 0; i < 60 && !target; i++) {
    await sleep(400);
    try {
      const list = await (await fetch(`http://127.0.0.1:${DBG}/json/list`)).json();
      target = list.find((t) => t.type === "page" && t.url.includes(String(PORT)));
    } catch { /* 还没起来 */ }
  }
  if (!target) throw new Error("Chrome 没能打开页面");
  const cdp = await CDP.attach(target.webSocketDebuggerUrl);
  // 页面报错/日志直接转发到控制台,免得白屏了还得猜
  cdp.ws.addEventListener("message", (e) => {
    const m = JSON.parse(e.data);
    if (m.method === "Runtime.consoleAPICalled") {
      console.log("  [页面]", m.params.args.map((a) => a.value ?? a.description ?? a.type).join(" "));
    } else if (m.method === "Runtime.exceptionThrown") {
      console.log("  [页面异常]", m.params.exceptionDetails?.exception?.description ?? JSON.stringify(m.params.exceptionDetails));
    }
  });
  await cdp.send("Runtime.enable");

  // 注入一台「已登录的主机」,跳过首装向导(等价于用户此前已连过电脑)
  await cdp.eval(`(() => {
    const base = 'http://127.0.0.1:${PORT}';
    localStorage.setItem('polaris.m.base', base);
    localStorage.setItem('polaris.m.token', 't0');
    localStorage.setItem('polaris.m.user', JSON.stringify({ username: 'test', role: 'owner' }));
    localStorage.setItem('polaris.m.hosts', JSON.stringify([{ id: 'h1', name: '我的电脑', base, addrs: [base], token: 't0', user: { username: 'test', role: 'owner' }, lastUsed: Date.now() }]));
    localStorage.setItem('polaris.m.activeHost', 'h1');
    localStorage.removeItem('polaris.m.project.h1');
    return true;
  })()`);
  await cdp.send("Page.enable");
  await cdp.send("Page.reload", { ignoreCache: true });

  // 等自动连接把界面带进对话页
  let ready = false;
  for (let i = 0; i < 40 && !ready; i++) {
    await sleep(300);
    ready = await cdp.eval(`!!document.querySelector('.dock textarea')`);
  }
  check("自动连上主机并进入对话页", ready);
  if (!ready) {
    console.log("  页面内容:", await cdp.eval(`document.body ? document.body.innerText.replace(/\\s+/g,' ').slice(0,400) : '(无 body)'`));
    console.log("  主机收到:", cmds(calls).join(", ") || "(一条都没有)");
    throw new Error("没进对话页");
  }

  // ── ① 说「打开北极星项目」:应就地切项目并在该项目下建会话,不惊动大模型 ──
  const a = await say(cdp, "打开北极星项目");
  check("① 切项目不发给大模型", !cmds(a).includes("chat_send"), `实际:${cmds(a).join(", ")}`);
  const created = a.find((c) => c.cmd === "conv_create_conversation");
  check("① 在选中的项目下建了会话", created?.args?.projectId === "p-alpha", `实际:${JSON.stringify(created?.args)}`);
  const bar = await cdp.eval(`(() => { const b = document.querySelector('.projbar'); return b ? b.innerText.replace(/\\s+/g,' ').trim() : null; })()`);
  check("① 顶栏显示在哪个项目/文件夹里干活", !!bar && bar.includes("北极星") && bar.includes("polaris-app"), `实际:${bar}`);
  await shot(cdp, "1-进了项目的对话页");

  // ── ② 接着正常说话:必须带主机发的真会话 id(cwd 才落在项目目录) ──
  const b = await say(cdp, "看看 README 里写了什么");
  const sent = b.find((c) => c.cmd === "chat_send");
  check("② 普通话照常发给大模型", !!sent);
  const cid = sent?.args?.args?.conversationId;
  check("② 用的是主机发的会话 id,不是本地 m-", typeof cid === "string" && cid.startsWith("c-host-"), `实际:${cid}`);

  // ── ③ 含「项目」二字但明显是正经请求的,不许被吞 ──
  const c = await say(cdp, "这个项目怎么跑起来");
  check("③ 正经问题不被口令误吞", cmds(c).includes("chat_send"), `实际:${cmds(c).join(", ")}`);

  // ── ④ 切到另一个项目:会话与 cwd 都得跟着换 ──
  const d = await say(cdp, "切到公司官网");
  const created2 = d.find((x) => x.cmd === "conv_create_conversation");
  check("④ 切到另一个项目并建新会话", created2?.args?.projectId === "p-beta", `实际:${JSON.stringify(created2?.args)}`);
  const e = await say(cdp, "把首页标题改成新的");
  const cid2 = e.find((x) => x.cmd === "chat_send")?.args?.args?.conversationId;
  check("④ 新会话与上一个项目的不是同一条", typeof cid2 === "string" && cid2.startsWith("c-host-") && cid2 !== cid, `实际:${cid2} vs ${cid}`);

  // ── ⑤ 应用直投没被项目口令抢走 ──
  const f = await say(cdp, "打开笔记");
  check("⑤「打开笔记」仍走应用直投", cmds(f).includes("app_pub_list") && !cmds(f).includes("chat_send"), `实际:${cmds(f).join(", ")}`);
  check("⑤ 真的进了应用全屏页", await cdp.eval(`!!document.querySelector('.live')`));

  // 退回对话页走真人路径:✕ 退出应用 → ‹ 返回。
  // (ChatScreen 是 v-show,在应用页时它的 DOM 还在 —— 直接点那个看不见的 ☰ 也"能过",
  //  但那不是用户能做到的操作,验收必须按真人路径来。)
  await cdp.eval(`document.querySelector('.live .abtn').click()`);
  await sleep(300);
  await cdp.eval(`document.querySelector('.apps .ahead .abtn').click()`);
  await sleep(400);
  check("⑤ 退出应用能回到对话页", await cdp.eval(`!document.querySelector('.live') && !!document.querySelector('.dock textarea')`));

  // ── ⑥ 「电脑项目」页:从抽屉进,列表要能看见项目和它绑的文件夹 ──
  await cdp.eval(`document.querySelector('.bar .orb-btn').click()`); // 左上角 ☰
  await sleep(500);
  await shot(cdp, "2-抽屉里的当前项目");
  const wentWork = await cdp.eval(`(() => {
    const b = [...document.querySelectorAll('.drawer .short')].find(x => x.innerText.includes('项目'));
    if (!b) return false;
    b.click();
    return true;
  })()`);
  await sleep(700);
  const rows = await cdp.eval(`[...document.querySelectorAll('.pcard')].map(c => c.innerText.replace(/\\s+/g,' ').trim())`);
  check("⑥ 抽屉能进电脑项目页", wentWork && Array.isArray(rows) && rows.length >= 2, `实际:${JSON.stringify(rows)}`);
  check("⑥ 列表显示项目名与绑定的文件夹", (rows ?? []).some((r) => r.includes("北极星") && r.includes("polaris-app")), `实际:${JSON.stringify(rows)}`);
  check("⑥ 当前所在项目有标记", (rows ?? []).some((r) => r.includes("当前") && r.includes("公司官网")), `实际:${JSON.stringify(rows)}`);
  await shot(cdp, "3-电脑项目页");

  // ── ⑦ 就地补绑工作目录:打错路径必须当场被主机拒掉,不能默默存下无效 cwd ──
  const markBad = calls.length;
  await cdp.eval(`(() => {
    const card = [...document.querySelectorAll('.pcard')].find(c => c.innerText.includes('北极星'));
    card.querySelector('.pedit').click();
    return true;
  })()`);
  await sleep(300);
  await cdp.eval(`(() => {
    const inp = document.querySelector('.pcard .finput');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
    setter.call(inp, 'D:\\\\这个目录并不存在');
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    [...document.querySelectorAll('.pcard .fbtn')].find(b => !b.classList.contains('ghost')).click();
    return true;
  })()`);
  await sleep(700);
  const badSet = calls.slice(markBad).find((c) => c.cmd === "conv_set_project_work_dir");
  check("⑦ 绑目录发到主机", !!badSet, `实际:${cmds(calls.slice(markBad)).join(", ")}`);
  check("⑦ 打错的路径没有被存下", PROJECTS.find((p) => p.id === "p-alpha").work_dir === "D:\\polaris\\polaris-app");
  const errToast = await cdp.eval(`(() => { const t = document.querySelector('.toast, .toasts'); return t ? t.innerText.replace(/\\s+/g,' ').trim() : null; })()`);
  check("⑦ 失败原因显示给用户", !!errToast && errToast.includes("不存在"), `实际:${errToast}`);

  // ── ⑧ 新建项目并直接进入 ──
  const markNew = calls.length;
  await cdp.eval(`document.querySelector('.padd').click()`);
  await sleep(300);
  await cdp.eval(`(() => {
    const inp = document.querySelector('.pcard.new .finput');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
    setter.call(inp, '试验田');
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    [...document.querySelectorAll('.pcard.new .fbtn')].find(b => !b.classList.contains('ghost')).click();
    return true;
  })()`);
  await sleep(900);
  const newCalls = calls.slice(markNew);
  const madeProj = newCalls.find((c) => c.cmd === "conv_create_project");
  check("⑧ 新建项目发到主机", madeProj?.args?.name === "试验田", `实际:${JSON.stringify(madeProj?.args)}`);
  const newConv = newCalls.find((c) => c.cmd === "conv_create_conversation");
  check("⑧ 建完直接进入新项目(会话建在它名下)", newConv?.args?.projectId === madeProj?.args?.id || !!newConv?.args?.projectId?.startsWith("p-new-"), `实际:${JSON.stringify(newConv?.args)}`);
  const backToChat = await cdp.eval(`!!document.querySelector('.dock textarea')`);
  check("⑧ 自动回到对话页", backToChat);

  console.log(bad ? `\n${bad} 条不符` : "\n全部通过");
} catch (e) {
  bad++;
  console.log("\n跑挂了: " + (e.stack ?? e));
} finally {
  try { chrome?.kill(); } catch {}
  server.close();
  await sleep(200);
  process.exit(bad ? 1 : 0);
}
