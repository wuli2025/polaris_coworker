<script setup lang="ts">
/**
 * L2「点哪讲哪」气泡 —— 全局挂一份,自己找目标。
 *
 * 目标组件只需在按钮上加 `data-coach="index"`,这里定时扫一遍 DOM:找到**当前露着、
 * 且没看过**的第一个标记,就贴上去弹一句(15 字以内)。点掉写已读,永不再扰。
 *
 * 为什么扫 DOM 而不是逐层传 prop:挂载点散在文件中心/对话/自动化/供应商坞四棵互不相干的
 * 组件树里,任何"从上往下传"都要改一路父组件。属性标记让接入成本降到「加一个 attribute」。
 *
 * 排在上手引导**之后**:五件事没走完时整个不出声 —— 同一颗按钮上叠两个气泡是灾难。
 */
import { ref, shallowRef, onMounted, onBeforeUnmount, nextTick, computed } from "vue";
import { X } from "@lucide/vue";
import { COACHMARKS, isSeen, markSeen, hasUnseen } from "../lib/coachmarks";
import { anchorTo, onScreen, type AnchorPos } from "../lib/anchor";
import { useWizardStore } from "../stores/wizard";
import { useStarterStore } from "../stores/starter";

const wiz = useWizardStore();
const starter = useStarterStore();

const key = ref<string | null>(null);
const anchor = shallowRef<HTMLElement | null>(null);
const bubble = ref<HTMLElement | null>(null);
const pos = ref<AnchorPos>({ top: 0, left: 0, arrow: 0, above: false });
const mark = computed(() => (key.value ? COACHMARKS[key.value] : null));

const W = 214;

function place() {
  const el = anchor.value;
  if (!el) return;
  pos.value = anchorTo(el.getBoundingClientRect(), W, bubble.value?.offsetHeight ?? 60);
}

function scan() {
  if (key.value) return; // 一次只弹一个,别糊用户一脸
  if (wiz.open || starter.visible) return; // 向导开着 / 上手五件事还没走完 → 先不出声
  if (!hasUnseen()) return;
  for (const el of document.querySelectorAll<HTMLElement>("[data-coach]")) {
    const k = el.dataset.coach;
    if (!k || !COACHMARKS[k] || isSeen(k) || !onScreen(el)) continue;
    key.value = k;
    anchor.value = el;
    void nextTick(() => {
      place();
      requestAnimationFrame(place);
    });
    return;
  }
}

function close() {
  if (key.value) markSeen(key.value);
  key.value = null;
  anchor.value = null;
  if (!hasUnseen()) stopTimer();
}

let timer: number | undefined;
function stopTimer() {
  if (timer !== undefined) {
    clearInterval(timer);
    timer = undefined;
  }
}

onMounted(() => {
  if (!hasUnseen()) return; // 全看过 → 从不挂表
  timer = window.setInterval(scan, 1600);
  window.addEventListener("resize", place);
  window.addEventListener("scroll", place, true);
});
onBeforeUnmount(() => {
  stopTimer();
  window.removeEventListener("resize", place);
  window.removeEventListener("scroll", place, true);
});
</script>

<template>
  <Teleport to="body">
    <Transition name="coach-fade">
      <div
        v-if="mark"
        ref="bubble"
        class="coach"
        :class="{ above: pos.above }"
        :style="{ top: pos.top + 'px', left: pos.left + 'px', width: W + 'px' }"
      >
        <span class="coach-arrow" :style="{ left: pos.arrow + 'px' }"></span>
        <div class="coach-hd">
          <span class="coach-t">{{ mark.title }}</span>
          <button class="coach-x" title="知道了" @click="close">
            <X :size="12" :stroke-width="2.2" />
          </button>
        </div>
        <div class="coach-b">{{ mark.body }}</div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.coach {
  position: fixed;
  z-index: 9500;
  padding: 8px 11px 9px;
  background: var(--panel);
  border: 1px solid var(--border-soft, var(--hairline));
  border-radius: 10px;
  box-shadow: var(--shadow-lg);
}
.coach-arrow {
  position: absolute;
  top: -5px;
  width: 9px;
  height: 9px;
  background: var(--panel);
  border-left: 1px solid var(--border-soft, var(--hairline));
  border-top: 1px solid var(--border-soft, var(--hairline));
  transform: translateX(-50%) rotate(45deg);
}
.coach.above .coach-arrow {
  top: auto;
  bottom: -5px;
  border-left: none;
  border-top: none;
  border-right: 1px solid var(--border-soft, var(--hairline));
  border-bottom: 1px solid var(--border-soft, var(--hairline));
}
.coach-hd {
  display: flex;
  align-items: center;
  gap: 6px;
}
.coach-t {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--ink, var(--text));
}
.coach-x {
  margin-left: auto;
  display: inline-flex;
  padding: 2px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--dim);
  cursor: pointer;
}
.coach-x:hover {
  color: var(--text);
  background: var(--selection-bg);
}
.coach-b {
  margin-top: 3px;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--muted);
}
.coach-fade-enter-active,
.coach-fade-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.coach-fade-enter-from,
.coach-fade-leave-to {
  opacity: 0;
  transform: translateY(-3px);
}
</style>
