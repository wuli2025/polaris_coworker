<script setup lang="ts">
/**
 * 「应用」页 —— 把电脑当服务器，手机跑它的网页端。
 *
 * 列出主机发布的本机应用，点开即在全屏 iframe 里用真实应用本体（后端在电脑上跑）。
 * 与远程桌面的差别：传的是 HTTP 报文不是像素 —— 清晰、省流、文字可选、触摸是原生触摸、
 * 版式按手机屏自适应；断线重连也不丢状态（状态在电脑上的应用里）。
 */
import { computed, onMounted, onUnmounted, ref } from "vue";
import Ico from "../components/Ico.vue";
import { go } from "../lib/nav";
import { list, open as openApp, pendingSlug, type PubApp } from "../lib/apps";
import { invoke } from "../lib/net";
import { openPreview } from "../lib/preview";
import { toast } from "../lib/toast";

const apps = ref<PubApp[]>([]);
const loading = ref(true);
const err = ref("");

/** 当前正在用的应用（全屏 iframe）。 */
const live = ref<{ app: PubApp; url: string } | null>(null);
/** 横屏：手机横过来时把顶栏收掉，内容占满 —— 这时的观感最接近电脑。 */
const landscape = ref(false);
/** 沉浸：藏起顶栏，点一下屏幕边缘唤回。 */
const immersive = ref(false);

let mq: MediaQueryList | null = null;
function onOrient() {
  landscape.value = !!mq?.matches;
  // 横过来自动进沉浸；转回竖屏自动退出，免得用户找不到返回键。
  immersive.value = landscape.value;
}

async function refresh() {
  loading.value = true;
  err.value = "";
  try {
    apps.value = (await list()) ?? [];
  } catch (e) {
    err.value = (e as Error).message || "读取失败";
  } finally {
    loading.value = false;
  }
}

async function launch(a: PubApp) {
  try {
    live.value = { app: a, url: await openApp(a.slug) };
  } catch (e) {
    toast((e as Error).message || "打开失败");
  }
}

/** 在 app 内直接读这套能力的说明页(主机把随包说明拷进白名单目录,走预览层打开)。 */
async function openDoc() {
  try {
    const p = await invoke<string>("beam_doc_path");
    openPreview(p);
  } catch (e) {
    toast((e as Error).message || "打不开说明");
  }
}

function closeLive() {
  live.value = null;
  immersive.value = false;
}

/** 在系统浏览器打开：WebView 里第三方 cookie 被拦时的兜底，也便于「添加到主屏幕」。 */
function openExternal() {
  if (live.value) window.open(live.value.url, "_blank");
}

async function toggleOrientation() {
  const o = screen.orientation as unknown as {
    lock?: (s: string) => Promise<void>;
    unlock?: () => void;
  };
  try {
    if (landscape.value) o.unlock?.();
    else await o.lock?.("landscape");
  } catch {
    toast("此设备不支持强制横屏，直接转手机即可");
  }
}

const title = computed(() => live.value?.app.name ?? "应用");

onMounted(async () => {
  mq = window.matchMedia("(orientation: landscape)");
  onOrient();
  mq.addEventListener("change", onOrient);
  await refresh();
  // 聊天里说「打开××」带过来的点名:列表就绪后直接拉起,不用再点一次。
  const s = pendingSlug.value;
  if (s) {
    pendingSlug.value = null;
    const a = apps.value.find((x) => x.slug === s);
    if (a) void launch(a);
    else toast("那个应用已下架,看看现有的吧");
  }
});
onUnmounted(() => mq?.removeEventListener("change", onOrient));
</script>

<template>
  <section class="apps">
    <!-- 列表 -->
    <template v-if="!live">
      <header class="ahead">
        <button class="abtn" title="返回" @click="go('chat')"><Ico name="chevron" :size="18" /></button>
        <h1>应用</h1>
        <button class="abtn" title="看说明" @click="openDoc"><Ico name="book" :size="17" /></button>
        <button class="abtn" title="刷新" @click="refresh"><Ico name="rotate" :size="17" /></button>
      </header>

      <p class="atip">
        这些是<b>电脑上正在跑</b>的应用。点开即用，计算全在电脑，手机只显示界面 ——
        比远程桌面清晰、省流，横屏更接近电脑。
      </p>

      <div v-if="loading" class="acenter muted">读取中…</div>
      <div v-else-if="err" class="acenter">
        <p class="aerr">{{ err }}</p>
        <button class="abig" @click="refresh">重试</button>
      </div>
      <div v-else-if="!apps.length" class="acenter">
        <Ico name="grid" :size="40" />
        <p class="muted">电脑还没发布任何应用</p>
        <p class="faint">
          去电脑端「互联 → 设备与授权 → 应用直投」扫一下本机端口再发布。<br />
          注意：Tauri / Electron 的<b>生产包默认不开 HTTP 端口</b>，那种应用投不了，
          需要开它的开发服务器。
        </p>
      </div>

      <ul v-else class="alist">
        <li v-for="a in apps" :key="a.slug">
          <button class="acard" @click="launch(a)">
            <span class="aico"><Ico name="grid" :size="20" /></span>
            <span class="atxt">
              <span class="aname">{{ a.name }}</span>
              <span class="ameta">端口 {{ a.port }}{{ a.note ? ` · ${a.note}` : "" }}</span>
            </span>
            <Ico name="chevron" :size="16" class="achev" />
          </button>
        </li>
      </ul>
    </template>

    <!-- 全屏使用中 -->
    <div v-else class="live" :class="{ immersive }">
      <header class="lbar">
        <button class="abtn" title="退出应用" @click="closeLive"><Ico name="close" :size="17" /></button>
        <span class="lname">{{ title }}</span>
        <button class="abtn" :class="{ on: landscape }" title="横竖屏" @click="toggleOrientation">
          <Ico name="rotate" :size="16" />
        </button>
        <button class="abtn" title="用系统浏览器打开" @click="openExternal">
          <Ico name="external" :size="16" />
        </button>
      </header>
      <iframe class="lframe" :src="live.url" allow="fullscreen; clipboard-read; clipboard-write"></iframe>
      <!-- 沉浸时顶栏收起，留一条细把手唤回，避免横屏被工具条吃掉一截 -->
      <button v-if="immersive" class="lhandle" @click="immersive = false"></button>
    </div>
  </section>
</template>

<style scoped>
.apps {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.ahead {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: calc(10px + var(--safe-top)) 12px 8px;
}
.ahead h1 {
  flex: 1;
  margin: 0;
  font-size: 17px;
  font-weight: 650;
}
.abtn {
  width: 36px;
  height: 36px;
  border-radius: 10px;
  background: var(--bg-elev2);
  color: var(--text-dim);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
.abtn.on {
  background: var(--accent);
  color: #fff;
}
.atip {
  margin: 0 14px 10px;
  font-size: 12.5px;
  line-height: 1.7;
  color: var(--text-dim);
}
.acenter {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 24px;
  text-align: center;
}
.aerr {
  color: #ff8f8f;
  margin: 0;
}
.abig {
  padding: 9px 20px;
  border-radius: 12px;
  background: var(--accent);
  color: #fff;
  font-size: 14px;
}
.alist {
  list-style: none;
  margin: 0;
  padding: 0 12px calc(16px + var(--safe-bottom));
  overflow: auto;
  flex: 1;
}
.acard {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 14px;
  margin-bottom: 10px;
  border-radius: 16px;
  background: var(--bg-elev2);
  color: inherit;
  text-align: left;
}
.aico {
  width: 40px;
  height: 40px;
  border-radius: 12px;
  display: grid;
  place-items: center;
  background: var(--accent);
  color: #fff;
  flex-shrink: 0;
}
.atxt {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.aname {
  font-size: 15px;
  font-weight: 600;
}
.ameta {
  font-size: 12px;
  color: var(--text-dim);
}
.achev {
  opacity: 0.4;
  transform: rotate(-90deg);
}

.live {
  position: fixed;
  inset: 0;
  z-index: 260;
  display: flex;
  flex-direction: column;
  background: var(--bg);
}
.lbar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: calc(8px + var(--safe-top)) 10px 6px;
  border-bottom: 1px solid var(--line);
  transition: margin-top 0.2s ease;
}
.live.immersive .lbar {
  display: none;
}
.lname {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 14px;
  font-weight: 600;
}
.lframe {
  flex: 1;
  width: 100%;
  border: none;
  background: #fff;
}
.lhandle {
  position: absolute;
  top: 0;
  left: 50%;
  transform: translateX(-50%);
  width: 88px;
  height: 16px;
  border-radius: 0 0 10px 10px;
  background: rgba(0, 0, 0, 0.35);
  z-index: 2;
}
</style>
