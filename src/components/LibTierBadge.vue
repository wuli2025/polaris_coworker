<script setup lang="ts">
/**
 * 知识库「速览 / 精读」状态徽标(见《新用户引导系统设计报告 v2》步骤 5 · 升级钩子)。
 *
 * 常驻左下角的玻璃小药丸,无论切到哪个视图都在,把「升级深度精读」做成随手可点的免费诱惑:
 *   - 速览版(quick)  → 一句话 + 「深度精读」钮:点一下即后台全自动跑完整流程(wiz.runDeepPipeline)。
 *   - 升级中(deepRunning) → 转圈 + 实时阶段文案(完整盘点 / 建索引 / 语义重聚命名)。
 *   - 精读版(deep)   → 打钩,数秒后可手动收起(本会话不再打扰)。
 *
 * 深度全流程进度同时也在右下角「全局任务中心」逐任务可见;这枚徽标是「态 + 一键升级入口」。
 */
import { ref, computed } from "vue";
import { Gauge, ShieldCheck, Microscope, X, LoaderCircle } from "@lucide/vue";
import { useWizardStore } from "../stores/wizard";
import { useAppStore } from "../stores/app";

const wiz = useWizardStore();
const app = useAppStore();
// 本会话手动收起(只针对「精读版」完成态;升级中/可升级态不允许收,免得用户找不到入口)。
const dismissed = ref(false);

const visible = computed(() => {
  if (wiz.open) return false; // 向导开着时它自己有升级卡,别叠
  if (wiz.libTier === "none") return false;
  if (wiz.libTier === "deep" && dismissed.value) return false;
  return true;
});

async function upgrade() {
  if (wiz.deepRunning || wiz.libTier === "deep") return;
  // 配好模型就立刻后台全自动跑(默认根=知识库+全盘);没配则武装 + 带去配 key,保存后自动开跑。
  const started = await wiz.requestDeep([]);
  if (!started) app.setView("sense_api");
}
</script>

<template>
  <transition name="ltb">
    <div
      v-if="visible"
      class="ltb"
      :class="{ quick: wiz.libTier === 'quick', running: wiz.deepRunning, deep: wiz.libTier === 'deep' && !wiz.deepRunning }"
    >
      <span class="ltb-ic">
        <LoaderCircle v-if="wiz.deepRunning" :size="15" class="spin" />
        <ShieldCheck v-else-if="wiz.libTier === 'deep'" :size="15" :stroke-width="1.9" />
        <Gauge v-else :size="15" :stroke-width="1.9" />
      </span>

      <div class="ltb-txt">
        <template v-if="wiz.deepRunning">
          <b>正在深度精读…</b>
          <span>{{ wiz.deepStageLabel }}</span>
        </template>
        <template v-else-if="wiz.libTier === 'deep'">
          <b>精读版已就绪</b>
          <span>语义检索 + AI 命名已全开,连 NAS 老资料都搜得准。</span>
        </template>
        <template v-else-if="wiz.pendingDeep">
          <b>已记下:配好模型就自动精读</b>
          <span>深度精读靠语义模型 —— 配好嵌入 key,我会立刻在后台跑完整流程。</span>
        </template>
        <template v-else>
          <b>当前:速览版</b>
          <span>重点都在、搜得到。要让 AI 把整库精读到极致吗?</span>
        </template>
      </div>

      <button v-if="wiz.libTier === 'quick' && !wiz.deepRunning" class="ltb-btn" @click="upgrade">
        <Microscope :size="13" :stroke-width="1.9" /> {{ wiz.pendingDeep ? "去配模型" : "深度精读" }}
      </button>
      <button v-else-if="wiz.libTier === 'deep' && !wiz.deepRunning" class="ltb-x" title="收起" @click="dismissed = true">
        <X :size="14" :stroke-width="2" />
      </button>
    </div>
  </transition>
</template>

<style scoped>
.ltb {
  --fast: #1fb573;
  --fast-2: #34d399;
  --deep: #7c6cff;
  --deep-2: #9d8cff;
  position: fixed;
  left: 18px;
  bottom: 18px;
  z-index: 9990;
  display: flex;
  align-items: center;
  gap: 11px;
  max-width: 360px;
  padding: 11px 13px;
  border-radius: 14px;
  border: 1px solid var(--border-soft);
  background: color-mix(in srgb, var(--panel) 86%, transparent);
  -webkit-backdrop-filter: blur(20px) saturate(1.4);
  backdrop-filter: blur(20px) saturate(1.4);
  box-shadow: var(--shadow-lg, 0 16px 48px rgba(0, 0, 0, 0.32));
}
.ltb.quick { border-color: color-mix(in srgb, var(--fast) 30%, var(--border-soft)); }
.ltb.running { border-color: color-mix(in srgb, var(--deep) 36%, transparent); }
.ltb.deep { border-color: color-mix(in srgb, var(--fast) 34%, var(--border-soft)); }
.ltb-ic {
  width: 32px;
  height: 32px;
  flex: none;
  border-radius: 9px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.ltb.quick .ltb-ic { color: var(--fast-2); background: color-mix(in srgb, var(--fast) 14%, transparent); }
.ltb.running .ltb-ic { color: var(--deep-2); background: color-mix(in srgb, var(--deep) 14%, transparent); }
.ltb.deep .ltb-ic { color: var(--fast-2); background: color-mix(in srgb, var(--fast) 14%, transparent); }
.ltb-txt { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.ltb-txt b { font-size: 12.5px; color: var(--ink); }
.ltb-txt span { font-size: 11px; color: var(--muted); line-height: 1.45; }
.ltb-btn {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 30px;
  padding: 0 12px;
  border-radius: 9px;
  cursor: pointer;
  border: 1px solid var(--deep);
  color: #fff;
  font-size: 12px;
  background: var(--deep);
  box-shadow: 0 5px 16px color-mix(in srgb, var(--deep) 26%, transparent);
  transition: filter 0.16s;
}
.ltb-btn:hover { filter: brightness(1.09); }
.ltb-x {
  flex: none;
  display: inline-flex;
  border: none;
  background: transparent;
  color: var(--dim);
  cursor: pointer;
  padding: 4px;
  border-radius: 7px;
}
.ltb-x:hover { color: var(--text); background: var(--selection-bg); }
.spin { animation: spin 0.9s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
.ltb-enter-active, .ltb-leave-active { transition: opacity 0.25s, transform 0.25s; }
.ltb-enter-from, .ltb-leave-to { opacity: 0; transform: translateY(10px); }
</style>
