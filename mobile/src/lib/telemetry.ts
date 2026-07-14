/**
 * 设备遥测上报(设备联盟 Phase 2)—— 手机把自己的真实资源报给主机,
 * 主机设备看板上这台手机的仪表就是真的。
 *
 * 口径如实(WebView 拿得到什么报什么,拿不到的不编):
 *  - cores      = navigator.hardwareConcurrency(真实逻辑核数)
 *  - mem_total  = navigator.deviceMemory(GB,浏览器量化值:0.25~8 档,真实但粗粒度)
 *  - disk_*     = storage.estimate()(**应用可用配额口径**,非整盘;真实但范围是本 App)
 *  - cpu_pct    = WebView 拿不到,不上报(主机端显示为缺项,不造假)
 *
 * 节律:登录后启动,前台每 REPORT_MS 一次;切后台停,回前台立即补一帧。
 */
import { deviceId, getBase, getToken } from "./net";

const REPORT_MS = 25_000;

let timer: ReturnType<typeof setInterval> | null = null;
let started = false;
let inFlight = false;
let curCtl: AbortController | null = null;

interface PhoneStats {
  cores?: number;
  mem_total?: number;
  disk_used?: number;
  disk_total?: number;
}

async function collect(): Promise<PhoneStats> {
  const s: PhoneStats = {};
  const cores = navigator.hardwareConcurrency;
  if (cores) s.cores = cores;
  const dm = (navigator as { deviceMemory?: number }).deviceMemory;
  if (dm) s.mem_total = Math.round(dm * 1024 ** 3);
  try {
    const est = await navigator.storage?.estimate?.();
    if (est?.quota) {
      s.disk_total = est.quota;
      s.disk_used = est.usage ?? 0;
    }
  } catch {
    /* 不支持就不报 */
  }
  return s;
}

async function reportOnce(): Promise<void> {
  const base = getBase();
  const token = getToken();
  if (!base || !token || document.visibilityState !== "visible") return;
  if (inFlight) return; // 单飞:半死连接下别叠新请求(codex #5)
  inFlight = true;
  const ctl = new AbortController();
  curCtl = ctl;
  const t = setTimeout(() => ctl.abort(), 8000); // 8s 超时,防 fetch 永久 pending
  try {
    const stats = await collect();
    if (!Object.keys(stats).length) return;
    await fetch(base + "/api/collab/telemetry", {
      method: "POST",
      headers: { "content-type": "application/json", Authorization: `Bearer ${token}` },
      body: JSON.stringify({ deviceId: deviceId(), stats }),
      signal: ctl.signal,
    });
  } catch {
    /* 掉线/超时/老主机没这端点:静默,下轮再试 */
  } finally {
    clearTimeout(t);
    inFlight = false;
    if (curCtl === ctl) curCtl = null;
  }
}

// 模块级只注册一次:start/stop 反复切换不会堆监听;started 闸控制是否真的上报。
if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (started && document.visibilityState === "visible") reportOnce(); // 回前台立即补一帧
  });
}

/** 登录后调用(幂等)。 */
export function startTelemetry(): void {
  if (started) return;
  started = true;
  reportOnce();
  timer = setInterval(reportOnce, REPORT_MS);
}

/** 登出/退出时停(可再 start)。 */
export function stopTelemetry(): void {
  if (timer) clearInterval(timer);
  timer = null;
  started = false;
  curCtl?.abort(); // 掐掉可能挂住的在途请求(codex #5)
  curCtl = null;
  inFlight = false;
}
