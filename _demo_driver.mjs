// CDP 驱动:注入协作登录态 → 切到协作看板 → 截图。Windows node 跑。
import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';

const CHROME = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const URL = 'http://localhost:8080';
const TOKEN = process.argv[2];
const OUT = process.argv[3] || 'D:\\polaris\\polaris-app\\_demo_shot_collab.png';
const CLICK_TEXT = process.argv[4] || '协作';

const sleep = (ms) => new Promise(r => setTimeout(r, ms));

const chrome = spawn(CHROME, [
  '--headless=new', '--disable-gpu', '--hide-scrollbars',
  '--remote-debugging-port=9333', '--remote-allow-origins=*',
  '--user-data-dir=C:\\Users\\mi\\AppData\\Local\\Temp\\polaris-cdp-' + Date.now(),
  '--window-size=1440,900', 'about:blank',
], { stdio: 'ignore' });

let ws, msgId = 0;
const pending = new Map();
function send(method, params = {}) {
  const id = ++msgId;
  ws.send(JSON.stringify({ id, method, params }));
  return new Promise(res => pending.set(id, res));
}

try {
  // 等调试端点就绪
  let target;
  for (let i = 0; i < 40; i++) {
    await sleep(300);
    try {
      const r = await fetch('http://127.0.0.1:9333/json');
      const list = await r.json();
      target = list.find(t => t.type === 'page');
      if (target) break;
    } catch {}
  }
  if (!target) throw new Error('未拿到 page target');

  ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result); pending.delete(m.id); }
  };

  await send('Page.enable');
  await send('Runtime.enable');

  // 首次导航以拿到 origin,再注入 localStorage
  await send('Page.navigate', { url: URL });
  await sleep(2500);
  await send('Runtime.evaluate', { expression: `
    localStorage.setItem('polaris.onboarded.v1','true');
    localStorage.setItem('polaris.collab.base.v1','');
    localStorage.setItem('polaris.collab.token.v1','${TOKEN}');
    localStorage.setItem('polaris.collab.deviceId.v1','dev-demo');
  ` });
  // 重载让协作 store 用 token 恢复会话
  await send('Page.reload', {});
  await sleep(4000);

  // 先关掉应用级「访问口令」弹窗(点「稍后」)
  await send('Runtime.evaluate', { expression: `
    (function(){
      const b = [...document.querySelectorAll('button')].find(e => e.textContent.trim()==='稍后' && e.offsetParent!==null);
      if (b){ b.click(); return 'dismissed'; } return 'no-modal';
    })()
  `}).then(r => console.log('modal:', r?.result?.value));
  await sleep(800);

  // 精确点侧栏「协作」叶子节点(title 精确匹配 或 文本恰为「协作」)
  await send('Runtime.evaluate', { expression: `
    (function(){
      const all = [...document.querySelectorAll('*')].filter(e => e.offsetParent!==null);
      let hit = all.find(e => (e.getAttribute&&e.getAttribute('title')||'').trim()==='${CLICK_TEXT}');
      if (!hit) hit = all.filter(e => e.children.length===0).find(e => e.textContent.trim()==='${CLICK_TEXT}');
      if (!hit) return 'notfound';
      // 点它或其可点击祖先
      let t = hit; for (let i=0;i<4 && t;i++){ if (t.tagName==='BUTTON'||t.getAttribute('role')==='button'||t.onclick){ break; } t = t.parentElement; }
      (t||hit).click();
      return 'clicked:'+(hit.getAttribute&&hit.getAttribute('title')||hit.textContent||'').trim().slice(0,10);
    })()
  `}).then(r => console.log('nav:', r?.result?.value));
  await sleep(3000);
  // 调试:浏览器上下文里直接打一枪 /api/collab/projects,确认 token 是否通
  await send('Runtime.evaluate', { awaitPromise: true, expression: `
    fetch('/api/collab/projects',{headers:{authorization:'Bearer ${TOKEN}'}})
      .then(r=>r.text().then(t=>'HTTP '+r.status+' '+t.slice(0,120)))
  `}).then(r => console.log('probe:', r?.result?.value));
  await sleep(2500); // 等看板重试/渲染

  const shot = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(OUT, Buffer.from(shot.data, 'base64'));
  console.log('SHOT SAVED:', OUT);
} catch (e) {
  console.error('DRIVER ERROR:', e.message);
} finally {
  try { ws?.close(); } catch {}
  chrome.kill();
  process.exit(0);
}
