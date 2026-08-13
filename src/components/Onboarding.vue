<script setup lang="ts">
/**
 * L0 首开 60 秒(见桌面《北极星 · 搭子式引导方案》)。
 *
 * 这一屏以前问的是「工作文件夹放哪」,还顺带解释了 raw/ output/ wiki/ 三层结构 ——
 * 而用户此刻**连这软件是干嘛的都不知道**,先被要求做一个技术决定。改成只问一句人话:
 * 你主要想让我帮你干哪类活?工作文件夹取推荐位置直接落库,想改的人点右下角小字。
 *
 * 选的这一项存进 starter store,右下角的搭子卡据此改招呼语 —— 让他一眼看出不是通用模板。
 */
import { onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { kb, isTauri } from "../tauri";
import { useStarterStore, type StarterIntent } from "../stores/starter";

const emit = defineEmits<{ (e: "done"): void }>();
const starter = useStarterStore();

const step = ref<1 | 2>(1);
const defaultRoot = ref("");
const draft = ref("");
const busy = ref(false);
const error = ref("");
const picked = ref<StarterIntent | null>(null);

const INTENTS: { key: StarterIntent; label: string; desc: string }[] = [
  { key: "files", label: "整理文件", desc: "电脑里堆了太多东西,想找得到、理得清" },
  { key: "write", label: "写东西", desc: "写稿、做方案、出材料,想有人搭把手" },
  { key: "code", label: "写代码", desc: "有项目在手,想让它读得懂、改得动" },
  { key: "all", label: "都试试", desc: "先随便看看,用着再说" },
];

/** 选完即进 —— 不再多问一屏。工作文件夹用推荐位置,想改的人走右下角小字。 */
async function choose(k: StarterIntent) {
  if (busy.value) return;
  picked.value = k;
  starter.setIntent(k);
  await finish();
}

onMounted(async () => {
  try {
    defaultRoot.value = await kb.defaultRoot();
    const cur = await kb.root();
    // 预填：优先当前已解析路径，否则默认
    draft.value = cur || defaultRoot.value;
  } catch {
    /* 浏览器模式下取不到，留空 */
  }
});

async function pickFolder() {
  if (!isTauri) {
    error.value = "浏览器预览模式不支持选择目录，正式应用里可用。";
    return;
  }
  error.value = "";
  const picked = await open({
    directory: true,
    multiple: false,
    title: "选择 Polaris 工作文件夹",
  });
  if (typeof picked === "string" && picked) {
    draft.value = picked;
  }
}

function useDefault() {
  draft.value = defaultRoot.value;
}

async function finish() {
  // 没填过就用推荐位置:首屏不该因为一个路径把人卡住(设置里随时能改)
  const v = draft.value.trim() || defaultRoot.value.trim();
  if (!v) {
    error.value = "还没能确定工作文件夹位置,请手动选一个。";
    step.value = 2;
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    await kb.setRoot(v);
    localStorage.setItem("polaris.onboarded.v1", "1");
    emit("done");
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="onboard">
    <div class="card">
      <!-- 顶部北极星徽记 -->
      <div class="badge">
        <span class="star"></span>
      </div>

      <!-- 第一步：一句人话 + 只问一个问题 -->
      <template v-if="step === 1">
        <h1 class="title">我是北极星</h1>
        <p class="lead">
          你电脑里堆着的那些文件，我可以帮你<strong>认一遍、找出来</strong>，
          再顺手<strong>替你办点事</strong>。资料全程留在你自己的机器上。
        </p>
        <p class="ask">先说一句：你主要想让我帮你干哪类活？</p>
        <div class="picks">
          <button
            v-for="it in INTENTS"
            :key="it.key"
            class="pick"
            :class="{ on: picked === it.key }"
            :disabled="busy"
            @click="choose(it.key)"
          >
            <span class="pick-t">{{ it.label }}</span>
            <span class="pick-d">{{ it.desc }}</span>
          </button>
        </div>
        <p v-if="error" class="err">{{ error }}</p>
        <p class="fine">
          <span v-if="busy">正在安顿工作文件夹…</span>
          <template v-else>
            选哪个都不影响你用别的功能，随时能改。
            <button class="link" @click="step = 2">换个工作文件夹</button>
          </template>
        </p>
      </template>

      <!-- 第二步（可选）：想自己指定工作文件夹的人才会来 -->
      <template v-else>
        <h1 class="title">把工作文件夹放在哪里？</h1>
        <p class="lead">
          Polaris 会在这个目录下维护三层结构：
          <code>raw/</code> 原始素材 · <code>output/</code> 生成成品 ·
          <code>wiki/</code> 知识维基。
        </p>
        <ul class="tips">
          <li>建议选一个<strong>容量充足、你会定期备份</strong>的位置。</li>
          <li>可以是网盘 / 同步盘里的目录，方便多台设备共享。</li>
          <li>之后随时能在「设置」里更改，旧目录不会被删除。</li>
        </ul>

        <div class="field-label">
          <span>工作文件夹路径</span>
          <button class="link" @click="useDefault" :disabled="busy">
            用推荐位置
          </button>
        </div>
        <div class="field">
          <input
            v-model="draft"
            class="path"
            :placeholder="defaultRoot || 'C:\\Users\\you\\Polaris\\PolarisKB'"
            :disabled="busy"
          />
          <button class="btn ghost" @click="pickFolder" :disabled="busy">浏览…</button>
        </div>
        <p class="rec" v-if="defaultRoot">
          推荐位置：<code>{{ defaultRoot }}</code>
        </p>

        <p v-if="error" class="err">{{ error }}</p>

        <div class="actions split">
          <button class="btn text" @click="step = 1" :disabled="busy">返回</button>
          <button class="btn primary" @click="finish" :disabled="busy">
            {{ busy ? "正在创建工作文件夹…" : "进入北极星" }}
          </button>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.onboard {
  position: fixed;
  inset: 0;
  z-index: 9998;
  display: flex;
  align-items: center;
  justify-content: center;
  background:
    radial-gradient(120% 80% at 50% -10%, #eef2f7 0%, var(--bg) 55%);
  padding: 40px;
}
.card {
  width: 100%;
  max-width: 560px;
  background: var(--panel);
  border: 1px solid var(--hairline);
  border-radius: 6px;
  box-shadow: var(--shadow-lg);
  padding: 42px 46px 38px;
  animation: cardIn 0.5s cubic-bezier(0.2, 0.7, 0.2, 1);
}
@keyframes cardIn {
  from { opacity: 0; transform: translateY(14px); }
  to { opacity: 1; transform: translateY(0); }
}

.badge {
  display: flex;
  justify-content: center;
  margin-bottom: 22px;
}
.star {
  position: relative;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--primary);
  box-shadow:
    0 0 0 4px var(--primary-soft),
    0 0 18px 4px rgba(44, 70, 97, 0.25);
}
.star::before,
.star::after {
  content: "";
  position: absolute;
  left: 50%;
  top: 50%;
  background: linear-gradient(var(--g, to right), transparent, var(--primary), transparent);
}
.star::before {
  width: 46px;
  height: 1.5px;
  transform: translate(-50%, -50%);
}
.star::after {
  width: 1.5px;
  height: 46px;
  transform: translate(-50%, -50%);
}

.title {
  font-family: var(--serif);
  font-size: 23px;
  font-weight: 600;
  letter-spacing: 2px;
  color: var(--ink);
  text-align: center;
  margin: 0 0 18px;
}
.lead {
  font-size: 13.5px;
  line-height: 2;
  color: var(--text-2);
  margin: 0 0 12px;
  letter-spacing: 0.3px;
}
.lead.dim {
  color: var(--muted);
}
.lead strong {
  color: var(--ink);
  font-weight: 600;
}
.tips {
  margin: 4px 0 24px;
  padding-left: 20px;
  font-size: 12.5px;
  line-height: 1.95;
  color: var(--text-2);
}
.tips strong {
  color: var(--primary-deep);
}
code {
  background: var(--code-bg);
  color: var(--code-text);
  padding: 1px 6px;
  border-radius: 3px;
  font-family: var(--mono);
  font-size: 11.5px;
}

.field-label {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  font-size: 11.5px;
  letter-spacing: 1px;
  color: var(--dim);
  font-family: var(--serif);
  margin-bottom: 6px;
}
.field {
  display: flex;
  gap: 8px;
}
.path {
  flex: 1;
  padding: 9px 11px;
  border: 1px solid var(--border);
  border-radius: 3px;
  font-family: var(--mono);
  font-size: 12px;
  background: var(--panel);
  color: var(--text);
}
.path:focus {
  outline: none;
  border-color: var(--primary);
}
.rec {
  font-size: 11.5px;
  color: var(--muted);
  margin: 10px 0 0;
}
.err {
  margin: 14px 0 0;
  padding: 8px 12px;
  border-radius: 3px;
  font-size: 12.5px;
  background: var(--vermilion-soft);
  color: var(--vermilion);
  border-left: 2px solid var(--vermilion);
}

.actions {
  display: flex;
  justify-content: center;
  margin-top: 30px;
}
.actions.split {
  justify-content: space-between;
  align-items: center;
}
.btn {
  padding: 9px 18px;
  border-radius: 3px;
  font-size: 13px;
  letter-spacing: 0.5px;
  border: 1px solid transparent;
}
.btn.primary {
  background: var(--btn-solid-bg);
  color: var(--btn-solid-text);
  border-color: var(--btn-solid-bg);
}
.btn.primary:hover:not(:disabled) {
  background: var(--primary);
  border-color: var(--primary);
}
.btn.ghost {
  background: transparent;
  border-color: var(--border);
  color: var(--text-2);
}
.btn.ghost:hover:not(:disabled) {
  border-color: var(--ink);
  color: var(--ink);
}
.btn.text {
  background: transparent;
  color: var(--muted);
}
.btn.text:hover:not(:disabled) {
  color: var(--ink);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
/* ── L0 首屏：一句人话 + 四选一 ── */
.ask {
  margin: 22px 0 14px;
  font-size: 14px;
  font-weight: 600;
  color: var(--ink);
  letter-spacing: 0.5px;
}
.picks {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.pick {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 13px 14px;
  text-align: left;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--panel);
  cursor: pointer;
  transition: border-color 0.16s, transform 0.16s, box-shadow 0.16s;
}
.pick:hover:not(:disabled) {
  border-color: var(--primary);
  transform: translateY(-1px);
  box-shadow: var(--shadow-sm);
}
.pick.on {
  border-color: var(--primary);
  box-shadow: 0 0 0 3px var(--primary-soft);
}
.pick:disabled {
  opacity: 0.55;
  cursor: default;
}
.pick-t {
  font-size: 14px;
  font-weight: 600;
  color: var(--ink);
}
.pick-d {
  font-size: 11.5px;
  line-height: 1.65;
  color: var(--muted);
}
.fine {
  margin: 18px 0 0;
  font-size: 11.5px;
  color: var(--muted);
  text-align: center;
}
.link {
  background: transparent;
  border: none;
  color: var(--primary);
  font-size: 11.5px;
  cursor: pointer;
  padding: 0;
}
.link:hover:not(:disabled) {
  text-decoration: underline;
}
.link:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
