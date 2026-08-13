<script setup lang="ts">
/**
 * 上手引导 —— **指着那颗按钮说一句话**,一次一件。
 *
 * 为什么不是右下角那张卡:卡片再好看也回答不了新手唯一的问题「到底点哪个」。
 * 所以这里只做两件事 ——
 *   ① 找到这一步对应的**真实按钮**(靠 data-spot 属性),把气泡贴上去、箭头指着它;
 *   ② 一句话说清点了会怎样,**15 字以内**。多一个字都是在替用户读说明书。
 *
 * 按钮不在当前页时不硬切视图(那是劫持),只在右下角留一条小签,用户点了才带路。
 * 用户真按下那颗按钮 → 自动进下一件,不用回来点「我做完了」。
 */
import { ref, shallowRef, computed, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import { X } from "@lucide/vue";
import { useStarterStore, type StarterKey } from "../stores/starter";
import { useAppStore, type ViewKey } from "../stores/app";
import { useWizardStore } from "../stores/wizard";
import { useWorkflowsStore } from "../stores/workflows";
import { useFileTasksStore } from "../stores/fileTasks";
import { anchorTo, onScreen, type AnchorPos } from "../lib/anchor";

const starter = useStarterStore();
const app = useAppStore();
const wiz = useWizardStore();
const workflows = useWorkflowsStore();
const tasks = useFileTasksStore();

/** 第 4 件替用户填进输入框的那句话 —— 按 L0 选的那类活换一句,问的都是「只有读过我硬盘才答得出」的东西。 */
const FIRST_QUESTION = computed(() => {
  switch (starter.intent) {
    case "write":
      return "我最近的资料里，有哪些能直接拿来写东西的？";
    case "code":
      return "我电脑里有哪些代码项目？各是干嘛的？";
    default:
      return "我们的资料库里有什么东西？分几类说说。";
  }
});

interface Step {
  key: StarterKey;
  n: number;
  /** 目标按钮的 data-spot 值 */
  spot: string;
  /** 按钮所在的视图(不在当前页时,小签点一下带过去) */
  view: ViewKey;
  /** 动作名:点了干嘛 */
  title: string;
  /** 一句话说清好处,15 字以内 */
  tip: string;
  /** 可选的一个小链接:能替用户做的就替他做(只此一个,点了才做,不自动覆盖他打的字) */
  action?: { label: string; run: () => void };
}

const STEPS: Step[] = [
  { key: "provider",  n: 1, spot: "provider", view: "chat",        title: "挑一家 AI",     tip: "填个 Key,它才能开口" },
  { key: "inventory", n: 2, spot: "wizard",   view: "file_center", title: "让它认识你的文件", tip: "扫一遍,几分钟就好" },
  { key: "index",     n: 3, spot: "index",    view: "file_center", title: "给文件贴标签",   tip: "贴完能按意思搜,挂后台" },
  {
    key: "ask", n: 4, spot: "ask", view: "chat", title: "问它一句试试", tip: "答案带出处,点得开原文件",
    action: { label: "替我填一句", run: () => workflows.insertText(FIRST_QUESTION.value) },
  },
  { key: "auto",      n: 5, spot: "morning",  view: "automation",  title: "打开每日晨报",   tip: "它每天自己整理一遍" },
];

const step = computed<Step | null>(() => {
  const k = starter.activeKey;
  return k ? STEPS.find((s) => s.key === k) ?? null : null;
});

// ── 找按钮、贴上去 ──
const target = shallowRef<HTMLElement | null>(null);
const bubble = ref<HTMLElement | null>(null);
const pos = ref<AnchorPos>({ top: 0, left: 0, arrow: 0, above: false });
const W = 232;

function place() {
  const el = target.value;
  if (!el) return;
  pos.value = anchorTo(el.getBoundingClientRect(), W, bubble.value?.offsetHeight ?? 76);
}

let clickOff: (() => void) | null = null;
/** 用户真按下那颗按钮 → 这件就算过了,不用回头点「我做完了」。 */
function bindTargetClick(el: HTMLElement, key: StarterKey) {
  clickOff?.();
  const h = () => window.setTimeout(() => starter.markDone(key), 60);
  el.addEventListener("click", h);
  clickOff = () => el.removeEventListener("click", h);
}

function clearTarget() {
  clickOff?.();
  clickOff = null;
  target.value?.removeAttribute("data-spot-on");
  target.value = null;
}

/** 扫一眼:这一步的按钮在不在当前页上、露没露出来。 */
function locate() {
  const s = step.value;
  if (!s || !starter.visible || wiz.open) {
    clearTarget();
    return;
  }
  const el = document.querySelector<HTMLElement>(`[data-spot="${s.spot}"]`);
  if (!el || !onScreen(el)) {
    clearTarget();
    return;
  }
  if (el === target.value) {
    place();
    return;
  }
  clearTarget();
  target.value = el;
  el.setAttribute("data-spot-on", ""); // 指着的同时让它自己也亮着
  bindTargetClick(el, s.key);
  void nextTick(() => {
    place();
    requestAnimationFrame(place); // 拿到真实高度后再校一次(上翻时尤其明显)
  });
}

/** 按钮不在这一页 → 小签点一下带路(不自动切视图,那是劫持)。 */
function goThere() {
  const s = step.value;
  if (!s) return;
  app.setView(s.view);
  window.setTimeout(locate, 420);
}

let timer: number | undefined;
onMounted(() => {
  locate();
  timer = window.setInterval(locate, 900); // 视图懒加载 / 弹层开合都会换 DOM,轮询最省心
  window.addEventListener("resize", place);
  window.addEventListener("scroll", place, true);
});
onBeforeUnmount(() => {
  clearTarget();
  if (timer !== undefined) clearInterval(timer);
  window.removeEventListener("resize", place);
  window.removeEventListener("scroll", place, true);
});
watch(() => [step.value?.key, app.view, wiz.open], () => void nextTick(locate));

// 能自己看出来的就别问用户:盘点跑完 / 索引开跑,自动勾掉对应那件。
watch(() => tasks.doneTick.inventory, (n, o) => { if (n > (o ?? 0)) starter.markDone("inventory"); });
watch(() => tasks.running.index, (on) => { if (on) starter.markDone("index"); });
</script>

<template>
  <template v-if="starter.visible && step && !wiz.open">
    <!-- 按钮就在眼前:气泡贴上去,箭头指着它 -->
    <Teleport to="body">
      <Transition name="sg-fade">
        <div
          v-if="target"
          ref="bubble"
          class="sg"
          :class="{ above: pos.above }"
          :style="{ top: pos.top + 'px', left: pos.left + 'px', width: W + 'px' }"
        >
          <span class="sg-arrow" :style="{ left: pos.arrow + 'px' }"></span>
          <div class="sg-hd">
            <span class="sg-n">{{ step.n }}</span>
            <span class="sg-t">{{ step.title }}</span>
            <button class="sg-x" title="这件明天再说" @click="starter.snoozeToday(step.key)">
              <X :size="12" :stroke-width="2.2" />
            </button>
          </div>
          <div class="sg-tip">
            {{ step.tip }}
            <button v-if="step.action" class="sg-act" @click="step.action.run()">
              {{ step.action.label }}
            </button>
          </div>
          <!-- 分步导航:翻页只挪光标,不勾掉 —— 跳过的那件转一圈还会回来 -->
          <div class="sg-nav">
            <span class="sg-prog">{{ starter.stepNo }}/{{ starter.total }}</span>
            <button class="sg-nav-b" :disabled="!starter.canFlip" @click="starter.prev()">
              上一步
            </button>
            <button class="sg-nav-b go" :disabled="!starter.canFlip" @click="starter.next()">
              下一步
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- 按钮不在这一页:右下角一条小签,点了才带路(绝不自动切视图,那是劫持) -->
    <!-- 不能用 v-else:上面那块被 Teleport 包着,兄弟链断了 -->
    <div v-if="!target" class="sg-pill-wrap">
      <button class="sg-pill" @click="goThere">
        <span class="sg-n sm">{{ step.n }}</span>
        <span>{{ step.title }}</span>
        <span class="sg-ar">→</span>
      </button>
      <!-- 这件先不做:同样只翻页,不勾掉 -->
      <button
        v-if="starter.canFlip"
        class="sg-pill-x wide"
        title="看下一件"
        @click="starter.next()"
      >
        下一步
      </button>
      <button class="sg-pill-x" title="不再提示(设置里可重看)" @click="starter.dismiss()">
        <X :size="11" :stroke-width="2.4" />
      </button>
    </div>
  </template>
</template>

<style scoped>
.sg {
  position: fixed;
  z-index: 9450;
  padding: 9px 11px 10px;
  background: var(--panel);
  border: 1px solid var(--border-soft, var(--hairline));
  border-radius: 10px;
  box-shadow: var(--shadow-lg);
}
.sg-arrow {
  position: absolute;
  top: -5px;
  width: 9px;
  height: 9px;
  background: var(--panel);
  border-left: 1px solid var(--border-soft, var(--hairline));
  border-top: 1px solid var(--border-soft, var(--hairline));
  transform: translateX(-50%) rotate(45deg);
}
.sg.above .sg-arrow {
  top: auto;
  bottom: -5px;
  border-left: none;
  border-top: none;
  border-right: 1px solid var(--border-soft, var(--hairline));
  border-bottom: 1px solid var(--border-soft, var(--hairline));
}
.sg-hd {
  display: flex;
  align-items: center;
  gap: 6px;
}
.sg-n {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 17px;
  height: 17px;
  border-radius: 50%;
  background: var(--gold, var(--primary));
  color: #fff;
  font-size: 10.5px;
  font-weight: 700;
  flex: none;
}
.sg-n.sm {
  width: 15px;
  height: 15px;
  font-size: 10px;
}
.sg-t {
  font-size: 13px;
  font-weight: 600;
  color: var(--ink, var(--text));
}
.sg-x {
  margin-left: auto;
  display: inline-flex;
  padding: 2px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--dim);
  cursor: pointer;
}
.sg-x:hover {
  color: var(--text);
  background: var(--selection-bg);
}
.sg-tip {
  margin-top: 3px;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--muted);
}

.sg-act {
  margin-left: 6px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--primary);
  font-size: 11.5px;
  cursor: pointer;
}
.sg-act:hover {
  text-decoration: underline;
}

/* 分步导航条:一条细分隔线压住,别跟正文抢注意力 */
.sg-nav {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  padding-top: 7px;
  border-top: 1px solid var(--border-soft, var(--hairline));
}
.sg-prog {
  margin-right: auto;
  font-size: 11px;
  color: var(--dim);
  font-variant-numeric: tabular-nums;
}
.sg-nav-b {
  padding: 3px 9px;
  border: 1px solid var(--border-soft, var(--hairline));
  border-radius: 6px;
  background: transparent;
  color: var(--text-2, var(--text));
  font-size: 11.5px;
  line-height: 1.5;
  cursor: pointer;
}
.sg-nav-b:hover:not(:disabled) {
  border-color: var(--primary);
  color: var(--primary);
}
.sg-nav-b.go {
  border-color: transparent;
  background: var(--gold, var(--primary));
  color: #fff;
}
.sg-nav-b.go:hover:not(:disabled) {
  filter: brightness(1.06);
  color: #fff;
}
/* 只剩这一件时翻页无处可去 —— 灰掉而不是藏掉,位置不跳 */
.sg-nav-b:disabled {
  opacity: 0.4;
  cursor: default;
}

.sg-pill-wrap {
  position: fixed;
  right: 18px;
  bottom: 84px;
  z-index: 9400;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.sg-pill-x {
  display: inline-flex;
  padding: 5px;
  border: 1px solid var(--border-soft, var(--hairline));
  border-radius: 50%;
  background: var(--panel);
  color: var(--dim);
  cursor: pointer;
  box-shadow: var(--shadow);
}
.sg-pill-x:hover {
  color: var(--text);
}
/* 小签上的「下一步」:跟那颗圆 X 同高,但是个字不是图标 */
.sg-pill-x.wide {
  padding: 0 11px;
  height: 30px;
  border-radius: 15px;
  font-size: 12px;
  align-items: center;
  /* 圆 X 那档灰在这儿会糊成一团看不清字,提一档到正文次级色 */
  color: var(--text-2, var(--text));
}
.sg-pill-x.wide:hover {
  border-color: var(--primary);
  color: var(--primary);
}
.sg-pill {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 30px;
  padding: 0 12px;
  border: 1px solid var(--border-soft, var(--hairline));
  border-radius: 20px;
  background: var(--panel);
  color: var(--text-2);
  font-size: 12.5px;
  cursor: pointer;
  box-shadow: var(--shadow);
}
.sg-pill:hover {
  border-color: var(--primary);
  color: var(--primary);
}
.sg-ar {
  color: var(--dim);
}

.sg-fade-enter-active,
.sg-fade-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.sg-fade-enter-from,
.sg-fade-leave-to {
  opacity: 0;
  transform: translateY(-3px);
}
</style>
