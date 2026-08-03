<script setup lang="ts">
/**
 * 远程文件全屏预览 —— 原文件在主机,这里只是"看到样子/体验功能":
 *  - html/svg → **fetch 取文本 + sandbox iframe srcdoc**(不能用 src= 直载:
 *    /api/file 对 html/svg/js 强制 `Content-Disposition: attachment`,WebView 见到就不渲染 = 白屏)
 *  - 图片/视频/音频 → 原生标签流式播(token 只能走 query)
 *  - 文本/代码/markdown → 拉取前 512KB 文字展示
 *  - 其他(pdf/压缩包等 WebView 啃不动的) → 引导用系统浏览器开
 *
 * 任何一步失败都要**把原因显示出来**(401 过期 / 403 权限或路径不在白名单 / 404 已删),
 * 以前 iframe 出错是静默白屏,用户只能看到"打不开"。
 *
 * 横竖屏:预览页右上角旋转钮 → screen.orientation.lock("landscape");
 * 关闭预览自动 unlock。锁不动(个别 ROM)时提示直接转手机(Manifest 未锁向,跟随重力)。
 *
 * 隔空同屏(投屏钮):主机把文件**包装成一页自包含网页**,手机与电脑同时渲染**同一份 HTML**
 * (而不是各画各的),滚动比例与指点因此能对齐。同步消息走 beam:sync 事件,底下是已有的
 * iroh 隧道 —— **不要求手机和电脑在同一 WiFi**,4G 也能投。详见 lib/beam.ts。
 */
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { previewPath, closePreview, openPreview } from "../lib/preview";
import { fileUrl, fileFetch } from "../lib/net";
import { renderMd } from "../lib/md";
import { toast } from "../lib/toast";
import Ico from "./Ico.vue";
import { AGENT, ME, on as beamOn, pack, send, type BeamDoc, type BeamMsg } from "../lib/beam";

type Kind = "html" | "image" | "video" | "audio" | "text" | "md" | "other";

const EXT_KIND: Record<string, Kind> = {
  // svg 也走 html 通道:它同样被打 attachment 头,<img src> 直载是白的
  html: "html", htm: "html", svg: "html",
  png: "image", jpg: "image", jpeg: "image", gif: "image", webp: "image", bmp: "image", ico: "image",
  mp4: "video", webm: "video", mov: "video", m4v: "video",
  mp3: "audio", wav: "audio", m4a: "audio", ogg: "audio", flac: "audio",
  md: "md", markdown: "md",
  txt: "text", log: "text", json: "text", js: "text", ts: "text", py: "text", rs: "text",
  css: "text", csv: "text", yml: "text", yaml: "text", toml: "text", xml: "text", sh: "text",
  vue: "text", sql: "text", ini: "text", conf: "text",
};

const path = computed(() => previewPath.value);
const name = computed(() => (path.value ?? "").split(/[\\/]/).pop() ?? "");
const kind = computed<Kind>(() => {
  const ext = (name.value.split(".").pop() ?? "").toLowerCase();
  return EXT_KIND[ext] ?? "other";
});
const url = computed(() => (path.value ? fileUrl(path.value) : ""));

const text = ref("");
const doc = ref(""); // html/svg 的源码,喂 iframe srcdoc
const loading = ref(false);
const err = ref("");
const landscape = ref(false);

const MAX_TEXT = 512 * 1024;
const MAX_DOC = 4 * 1024 * 1024;

/** 把失败翻成人话:光说"打不开"没法排查,要指到具体是权限还是路径。 */
async function reason(res: Response): Promise<string> {
  const body = (await res.text().catch(() => "")).trim().slice(0, 200);
  if (res.status === 401) return "登录已过期,请重新登录主机";
  if (res.status === 403) return body || "主机拒绝访问(权限或路径不在允许范围)";
  if (res.status === 404) return body || "文件在主机上不存在(可能已被删除或移动)";
  return body ? `${body}(HTTP ${res.status})` : `读取失败(HTTP ${res.status})`;
}

watch(path, async (p) => {
  text.value = "";
  doc.value = "";
  err.value = "";
  if (!p) {
    // 关闭时松开横屏锁
    if (landscape.value) unlockOrientation();
    return;
  }
  const k = kind.value;
  if (k !== "text" && k !== "md" && k !== "html") return;
  loading.value = true;
  try {
    const res = await fileFetch(p);
    if (!res.ok) {
      err.value = await reason(res);
      return;
    }
    const raw = await res.text();
    if (k === "html") {
      doc.value =
        raw.length > MAX_DOC
          ? `<pre style="padding:16px;font:13px/1.5 monospace">文件过大(${Math.round(
              raw.length / 1024
            )}KB),App 内不渲染,请用浏览器打开</pre>`
          : raw;
    } else {
      text.value =
        raw.length > MAX_TEXT ? raw.slice(0, MAX_TEXT) + "\n\n…(仅预览前 512KB)" : raw;
    }
  } catch (e) {
    err.value = (e as Error).message || "无法连接主机";
  } finally {
    loading.value = false;
  }
});

/** 图片/音视频是元素自己加载的,失败只有 onerror,没有状态码可拿。 */
function mediaFailed() {
  err.value = "加载失败 —— 可能是登录过期、主机拒绝访问,或文件已不在主机上";
}

interface LockableOrientation {
  lock?: (o: string) => Promise<void>;
  unlock?: () => void;
}
function orientationApi(): LockableOrientation {
  return (screen.orientation ?? {}) as unknown as LockableOrientation;
}
async function toggleOrientation() {
  const o = orientationApi();
  try {
    if (landscape.value) {
      o.unlock?.();
      landscape.value = false;
    } else {
      if (!o.lock) throw new Error("unsupported");
      await o.lock("landscape");
      landscape.value = true;
    }
  } catch {
    toast("此设备不支持强制横屏,直接旋转手机即可(已支持自动转向)");
  }
}
function unlockOrientation() {
  try {
    orientationApi().unlock?.();
  } catch {
    /* ignore */
  }
  landscape.value = false;
}

function close() {
  unlockOrientation();
  if (beamOnAir.value) stopBeam();
  closePreview();
}
function openExternal() {
  if (url.value) window.open(url.value, "_blank");
}

// ── 隔空同屏 ───────────────────────────────────────────────────────────
// 开着同屏时,手机显示的不再是各类型分头渲染的预览,而是与电脑**同一份**打包网页 ——
// 这是「和手机上打开一样的效果」的前提:两边不是同一份 HTML,滚动比例就没有意义。

const beamOnAir = ref(false);
const beamDoc = ref<BeamDoc | null>(null);
const beamBusy = ref(false);
const beamFrame = ref<HTMLIFrameElement | null>(null);
/** 收到远端滚动后的抑制窗口:不设就会两端来回抖。 */
let lockUntil = 0;
let lastSent = 0;
let offBeam: (() => void) | null = null;

/** 手机主动投:打包 → 本地切到同屏视图 → 广播 open。 */
async function toggleBeam() {
  if (beamOnAir.value) {
    stopBeam();
    return;
  }
  const p = path.value;
  if (!p) return;
  if (!(await enterBeam(p))) return;
  // 看 delivered:连的是无桌面订阅者的主机(如 NAS server 版)时,别假报「已投到电脑」。
  const r = await send({ act: "open", path: p, name: name.value });
  toast(
    r?.delivered
      ? "已投到电脑 —— 两边看同一页,滚动会互相跟随"
      : "已在本机打开;电脑端未连上,暂时收不到"
  );
}

// 打包代际:连投两个文件时慢的那次后返回会覆盖快的 —— 代际号丢弃迟到结果。
let beamGen = 0;
/** 打包并进入同屏视图。返回是否成功。 */
async function enterBeam(p: string): Promise<boolean> {
  const gen = ++beamGen;
  beamBusy.value = true;
  try {
    const packed = await pack(p);
    if (gen !== beamGen) return false; // 已被更晚的 open 取代
    beamDoc.value = packed;
    beamOnAir.value = true;
    return true;
  } catch (e) {
    if (gen === beamGen) toast((e as Error).message || "投屏失败");
    return false;
  } finally {
    if (gen === beamGen) beamBusy.value = false;
  }
}

function stopBeam(notify = true) {
  if (beamOnAir.value && notify) void send({ act: "close" });
  beamOnAir.value = false;
  beamDoc.value = null;
}

/**
 * 对端(电脑或另一台手机)发来的动作。
 *
 * 监听常驻而非只在同屏时挂 —— 否则电脑侧的「投到手机」永远没人接:
 * 手机得先自己开着同屏才收得到,那就不叫「投过来」了。
 */
let remoteApplying = false;
async function onBeamMsg(m: BeamMsg) {
  if (!m || m.from === ME) return;
  // 对端发起:自动打开那个文件并进入同屏(等价于手机自己点了投屏)。
  if (m.act === "open") {
    if (!m.path) return;
    remoteApplying = true;
    try {
      openPreview(m.path);
      if (await enterBeam(m.path)) toast("电脑发来同屏 —— 两边看同一页");
    } finally {
      remoteApplying = false;
    }
    return;
  }
  if (!beamOnAir.value) return;
  if (m.act === "close") {
    stopBeam(false);
    toast("对方已结束同屏");
  } else if (m.act === "scroll") {
    lockUntil = Date.now() + 400;
    toFrame({ act: "scroll", r: m.r ?? 0 });
  } else if (m.act === "mark") {
    toFrame({ act: "mark", rx: m.rx ?? 0, ry: m.ry ?? 0 });
  } else if (m.act === "zoom") {
    toFrame({ act: "zoom", z: m.z ?? 1 });
  } else if (m.act === "ping") {
    void send({ act: "pong" });
  }
}

function toFrame(m: Record<string, unknown>) {
  try {
    beamFrame.value?.contentWindow?.postMessage({ ...m, __beam: AGENT }, "*");
  } catch {
    /* iframe 未就绪,下一条补上 */
  }
}

/** 打包页里的代理脚本上报 → 转发给电脑。 */
function onFrameMsg(e: MessageEvent) {
  const d = e.data as Record<string, unknown> | null;
  if (!d || d.__beam !== AGENT || !beamOnAir.value) return;
  // 无 frame 即丢弃(不是「有 frame 才校验」):别让别的 iframe 借道上总线。
  if (!beamFrame.value || e.source !== beamFrame.value.contentWindow) return;
  if (d.act === "ready") {
    // 页面解析完再注入初始字号,不靠定时器赌就绪。
    toFrame({ act: "zoom", z: 1 });
  } else if (d.act === "scroll") {
    if (Date.now() < lockUntil) return;
    const now = Date.now();
    if (now - lastSent < 90) return; // 节流:滚动是连续事件,别把总线灌满
    lastSent = now;
    void send({ act: "scroll", r: Number(d.r) || 0 });
  } else if (d.act === "mark") {
    void send({ act: "mark", rx: Number(d.rx) || 0, ry: Number(d.ry) || 0 });
  }
}

/**
 * 横屏沉浸:同屏时手机一横过来就把顶栏/页脚收掉,内容占满整屏。
 *
 * 打包页本身在宽视口下会自动切成**电脑版式**(beam.rs 的 `@media (min-width:700px)`),
 * 所以横屏 + 沉浸 = 手上拿到的就是电脑上那一页。点一下顶部细把手唤回工具条。
 */
const isLandscape = ref(false);
const immersive = ref(false);
let mq: MediaQueryList | null = null;
function onOrient() {
  isLandscape.value = !!mq?.matches;
  immersive.value = isLandscape.value && beamOnAir.value;
}

onMounted(() => {
  window.addEventListener("message", onFrameMsg);
  // 常驻订阅:本组件在 App 里只挂一次,同屏邀请随时可能从电脑那边过来。
  offBeam = beamOn((m) => void onBeamMsg(m));
  mq = window.matchMedia("(orientation: landscape)");
  onOrient();
  mq.addEventListener("change", onOrient);
});
onUnmounted(() => {
  window.removeEventListener("message", onFrameMsg);
  mq?.removeEventListener("change", onOrient);
  offBeam?.();
  offBeam = null;
});

// 进/出同屏都要重算沉浸态:竖屏进同屏不该直接藏工具条。
watch(beamOnAir, () => onOrient());

// 换文件即退出同屏:旧的同步比例套到新文件上毫无意义。
// 对端发起的 open 会先换 path 再进同屏,那条路径由 remoteApplying 放行。
watch(path, () => {
  if (beamOnAir.value && !remoteApplying) stopBeam();
});
</script>

<template>
  <transition name="fade">
    <div v-if="path" class="pv" :class="{ immersive }">
      <!-- 沉浸时唤回工具条的细把手(横屏同屏下顶栏是收起的) -->
      <button v-if="immersive" class="pv-handle" @click="immersive = false"></button>
      <header class="pv-bar">
        <button class="pv-btn" title="关闭" @click="close"><Ico name="close" :size="17" /></button>
        <div class="pv-name" :title="path ?? ''">{{ name }}</div>
        <div class="pv-acts">
          <button
            class="pv-btn"
            :class="{ on: beamOnAir }"
            :disabled="beamBusy"
            :title="beamOnAir ? '结束同屏' : '投到电脑(隔空同屏)'"
            @click="toggleBeam"
          >
            <Ico name="cast" :size="17" />
          </button>
          <button class="pv-btn" :class="{ on: landscape }" title="横竖屏切换" @click="toggleOrientation">
            <Ico name="rotate" :size="17" />
          </button>
          <button class="pv-btn" title="浏览器打开" @click="openExternal">
            <Ico name="external" :size="17" />
          </button>
        </div>
      </header>

      <div class="pv-body">
        <!-- 同屏中:与电脑渲染同一份打包网页(优先级最高,盖住各类型分头渲染) -->
        <iframe
          v-if="beamOnAir && beamDoc"
          ref="beamFrame"
          class="pv-frame"
          :srcdoc="beamDoc.html"
          sandbox="allow-scripts allow-forms allow-popups allow-modals"
          allow="fullscreen"
        ></iframe>
        <div v-else-if="beamBusy" class="pv-center"><p class="muted">正在把文件包装成网页…</p></div>

        <!-- 出错:把原因摆出来,别白屏 -->
        <div v-else-if="err" class="pv-center">
          <div class="pv-other">
            <div class="pv-big">⚠️</div>
            <p class="pv-err">{{ err }}</p>
            <p class="faint pv-path">{{ path }}</p>
            <button class="btn ghost" @click="openExternal">用浏览器打开试试</button>
          </div>
        </div>
        <div v-else-if="loading" class="pv-center"><p class="muted">加载中…</p></div>

        <!-- html/svg:srcdoc + sandbox(不给 allow-same-origin = 页内脚本够不着主机接口) -->
        <iframe
          v-else-if="kind === 'html'"
          class="pv-frame"
          :srcdoc="doc"
          sandbox="allow-scripts allow-forms allow-popups allow-modals"
          allow="fullscreen"
        ></iframe>
        <div v-else-if="kind === 'image'" class="pv-center">
          <img class="pv-img" :src="url" :alt="name" @error="mediaFailed" />
        </div>
        <div v-else-if="kind === 'video'" class="pv-center">
          <video
            class="pv-media"
            :src="url"
            controls
            autoplay
            playsinline
            @error="mediaFailed"
          ></video>
        </div>
        <div v-else-if="kind === 'audio'" class="pv-center">
          <div class="pv-audio">
            <div class="pv-big">🎵</div>
            <audio :src="url" controls autoplay @error="mediaFailed"></audio>
          </div>
        </div>
        <div v-else-if="kind === 'md'" class="pv-text md" v-html="renderMd(text)"></div>
        <pre v-else-if="kind === 'text'" class="pv-text">{{ text }}</pre>
        <div v-else class="pv-center">
          <div class="pv-other">
            <div class="pv-big">📦</div>
            <p class="muted">这个格式 App 内看不了</p>
            <button class="btn" @click="openExternal">用浏览器打开</button>
          </div>
        </div>
      </div>

      <p class="pv-foot faint">
        {{
          beamOnAir
            ? "隔空同屏中 · 电脑正在看同一页,滚动与指点实时同步"
            : "远程预览 · 文件在主机上,不占手机存储"
        }}
      </p>
    </div>
  </transition>
</template>

<style scoped>
.pv {
  position: fixed;
  inset: 0;
  z-index: 300;
  background: var(--bg);
  display: flex;
  flex-direction: column;
}
.pv-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: calc(10px + var(--safe-top)) 10px 8px;
  border-bottom: 1px solid var(--line);
}
.pv-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 600;
  font-size: 14px;
}
.pv-acts {
  display: flex;
  gap: 4px;
}
.pv-btn {
  width: 36px;
  height: 36px;
  border-radius: 10px;
  background: var(--bg-elev2);
  color: var(--text-dim);
  font-size: 16px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.pv-btn:disabled {
  opacity: 0.45;
}
.pv-btn.on {
  background: var(--accent);
  color: #fff;
}
.pv-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.pv-frame {
  flex: 1;
  width: 100%;
  border: none;
  background: #fff;
}
.pv-center {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
  overflow: auto;
}
.pv-img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}
.pv-media {
  width: 100%;
  max-height: 100%;
}
.pv-audio,
.pv-other {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
}
.pv-big {
  font-size: 52px;
}
.pv-err {
  margin: 0;
  font-size: 14px;
  line-height: 1.6;
  max-width: 300px;
}
.pv-path {
  margin: 0;
  font-size: 11px;
  word-break: break-all;
  max-width: 300px;
  -webkit-user-select: text;
  user-select: text;
}
.pv-text {
  flex: 1;
  overflow: auto;
  margin: 0;
  padding: 14px;
  font-size: 13px;
  -webkit-user-select: text;
  user-select: text;
  white-space: pre-wrap;
  word-break: break-word;
}
.pv-text.md {
  white-space: normal;
}
.pv-foot {
  text-align: center;
  padding: 4px 0 calc(6px + var(--safe-bottom));
  border-top: 1px solid var(--line);
}
/* 横屏沉浸:顶栏与页脚收掉,整屏交给内容(打包页在宽视口下自动是电脑版式) */
.pv.immersive .pv-bar,
.pv.immersive .pv-foot {
  display: none;
}
.pv-handle {
  position: absolute;
  top: 0;
  left: 50%;
  transform: translateX(-50%);
  width: 88px;
  height: 16px;
  border-radius: 0 0 10px 10px;
  background: rgba(0, 0, 0, 0.35);
  z-index: 5;
}
</style>
