<script setup lang="ts">
/**
 * 技能选择底部面板(豆包式):多选,勾中的技能随下一条消息下发(chat_send skillIds)。
 * 列表来自主机 list_skills(只展示已安装);老主机不支持时给出提示。
 */
import { watch } from "vue";
import {
  skills,
  skillState,
  chosenSkillIds,
  toggleSkill,
  loadSkills,
} from "../lib/pickers";

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{ "update:modelValue": [boolean] }>();

watch(
  () => props.modelValue,
  (open) => {
    if (open) loadSkills();
  }
);

function close() {
  emit("update:modelValue", false);
}
</script>

<template>
  <transition name="fade">
    <div v-if="props.modelValue" class="scrim" @click="close"></div>
  </transition>
  <transition name="sheet">
    <div v-if="props.modelValue" class="sheet">
      <div class="grabber"></div>
      <div class="head">
        <h3>选择技能</h3>
        <button v-if="chosenSkillIds.length" class="clear" @click="chosenSkillIds = []">
          清空({{ chosenSkillIds.length }})
        </button>
      </div>
      <p class="faint tip">勾选的技能会随下一条消息一起生效,可多选。</p>

      <div class="list">
        <p v-if="skillState === 'loading' && !skills.length" class="faint empty">加载中…</p>
        <p v-else-if="skillState === 'unsupported'" class="faint empty">
          当前主机版本暂不支持技能列表,升级主机后可用。
        </p>
        <p v-else-if="!skills.length" class="faint empty">主机上还没有安装技能。</p>
        <div
          v-for="s in skills"
          :key="s.id"
          class="row"
          :class="{ on: chosenSkillIds.includes(s.id) }"
          @click="toggleSkill(s.id)"
        >
          <div class="meta">
            <div class="name">{{ s.name }}</div>
            <div class="desc faint">{{ s.description }}</div>
          </div>
          <span class="check" :class="{ on: chosenSkillIds.includes(s.id) }">✓</span>
        </div>
      </div>

      <button class="btn full done" @click="close">完成</button>
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
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
h3 {
  margin: 0;
  font-size: 16px;
}
.clear {
  font-size: 13px;
  color: var(--accent);
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
  padding: 30px 0;
}
.row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 11px 10px;
  border-radius: 12px;
  cursor: pointer;
}
.row.on {
  background: var(--accent-soft);
}
.row:active {
  background: var(--bg-elev2);
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
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}
.check {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 1.5px solid var(--line-hi);
  color: transparent;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  flex-shrink: 0;
  transition: background 0.15s, color 0.15s;
}
.check.on {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.done {
  margin-top: 12px;
  flex-shrink: 0;
}
.full {
  width: 100%;
}
</style>
