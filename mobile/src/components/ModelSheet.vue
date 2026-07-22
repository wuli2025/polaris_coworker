<script setup lang="ts">
/**
 * 模型(供应商)切换底部面板(豆包式):单选。
 * Auto = 跟随主机全局当前供应商;选具体一家 = 本对话逐条钉死这家(chat_send providerId,
 * 与桌面「每对话各用各 API」同一机制,不改主机全局配置)。
 */
import { computed, watch } from "vue";
import {
  providers,
  providerState,
  hostCurrentProvider,
  chosenProviderId,
  loadProviders,
} from "../lib/pickers";

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{ "update:modelValue": [boolean] }>();

watch(
  () => props.modelValue,
  (open) => {
    if (open) loadProviders();
  }
);

// 已配 key 的排前面;未配的殿后置灰(点了也连不上,但让用户知道有这家)
const sorted = computed(() =>
  [...providers.value].sort((a, b) => Number(b.hasKey ?? false) - Number(a.hasKey ?? false))
);
const autoName = computed(
  () => providers.value.find((p) => p.id === hostCurrentProvider.value)?.name ?? ""
);

function close() {
  emit("update:modelValue", false);
}
function pick(id: string, enabled: boolean) {
  if (!enabled) return;
  chosenProviderId.value = id;
  close();
}
</script>

<template>
  <transition name="fade">
    <div v-if="props.modelValue" class="scrim" @click="close"></div>
  </transition>
  <transition name="sheet">
    <div v-if="props.modelValue" class="sheet">
      <div class="grabber"></div>
      <h3>切换模型</h3>
      <p class="faint tip">只对本机会话生效,不影响主机全局与其他人。</p>

      <div class="list">
        <div class="row" :class="{ on: chosenProviderId === 'auto' }" @click="pick('auto', true)">
          <span class="dot auto">A</span>
          <div class="meta">
            <div class="name">Auto · 跟随主机</div>
            <div class="desc faint">{{ autoName ? `当前:${autoName}` : "用主机全局当前供应商" }}</div>
          </div>
          <span class="radio" :class="{ on: chosenProviderId === 'auto' }"></span>
        </div>

        <p v-if="providerState === 'loading' && !providers.length" class="faint empty">加载中…</p>
        <p v-else-if="providerState === 'unsupported'" class="faint empty">
          当前主机版本暂不支持模型列表,升级主机后可用。
        </p>
        <div
          v-for="p in sorted"
          :key="p.id"
          class="row"
          :class="{ on: chosenProviderId === p.id, off: !p.hasKey }"
          @click="pick(p.id, !!p.hasKey)"
        >
          <span class="dot" :style="p.color ? { background: p.color } : {}">{{
            p.name.slice(0, 1)
          }}</span>
          <div class="meta">
            <div class="name">{{ p.name }}</div>
            <div class="desc faint">
              {{ p.hasKey ? p.category || "已配置" : "未配置 · 请先在桌面端填 key" }}
            </div>
          </div>
          <span class="radio" :class="{ on: chosenProviderId === p.id }"></span>
        </div>
      </div>
    </div>
  </transition>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  z-index: 150;
}
.sheet {
  position: fixed;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 160;
  background: var(--bg-elev);
  -webkit-backdrop-filter: var(--blur);
  backdrop-filter: var(--blur);
  border-top-left-radius: 22px;
  border-top-right-radius: 22px;
  border-top: 1px solid var(--line-hi);
  padding: 8px 18px calc(16px + var(--safe-bottom));
  display: flex;
  flex-direction: column;
  max-height: 72vh;
}
.grabber {
  width: 40px;
  height: 4px;
  border-radius: 2px;
  background: var(--line);
  margin: 6px auto 12px;
  flex-shrink: 0;
}
h3 {
  margin: 0;
  font-size: 16px;
}
.tip {
  margin: 6px 0 10px;
}
.list {
  flex: 1;
  overflow-y: auto;
  min-height: 120px;
}
.empty {
  text-align: center;
  padding: 24px 0;
}
.row {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 11px 10px;
  border-radius: 12px;
  cursor: pointer;
}
.row.on {
  background: var(--accent-soft);
}
.row.off {
  opacity: 0.45;
}
.row:active {
  background: var(--bg-elev2);
}
.dot {
  width: 34px;
  height: 34px;
  border-radius: 10px;
  background: var(--accent);
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  font-size: 15px;
  flex-shrink: 0;
}
.dot.auto {
  background: linear-gradient(135deg, var(--accent), var(--accent-2));
}
.meta {
  flex: 1;
  min-width: 0;
}
.name {
  font-size: 14.5px;
  font-weight: 500;
}
.desc {
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.radio {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 1.5px solid var(--line-hi);
  flex-shrink: 0;
  position: relative;
}
.radio.on {
  border-color: var(--accent);
}
.radio.on::after {
  content: "";
  position: absolute;
  inset: 4px;
  border-radius: 50%;
  background: var(--accent);
}
</style>
