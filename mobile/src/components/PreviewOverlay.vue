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
 */
import { computed, ref, watch } from "vue";
import { previewPath, closePreview } from "../lib/preview";
import { fileUrl, fileFetch } from "../lib/net";
import { renderMd } from "../lib/md";
import { toast } from "../lib/toast";

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
  closePreview();
}
function openExternal() {
  if (url.value) window.open(url.value, "_blank");
}
</script>

<template>
  <transition name="fade">
    <div v-if="path" class="pv">
      <header class="pv-bar">
        <button class="pv-btn" title="关闭" @click="close">✕</button>
        <div class="pv-name" :title="path ?? ''">{{ name }}</div>
        <div class="pv-acts">
          <button class="pv-btn" :class="{ on: landscape }" title="横竖屏切换" @click="toggleOrientation">⟳</button>
          <button class="pv-btn" title="浏览器打开" @click="openExternal">↗</button>
        </div>
      </header>

      <div class="pv-body">
        <!-- 出错:把原因摆出来,别白屏 -->
        <div v-if="err" class="pv-center">
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

      <p class="pv-foot faint">远程预览 · 文件在主机上,不占手机存储</p>
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
  padding: calc(8px + var(--safe-top)) 10px 8px;
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
</style>
