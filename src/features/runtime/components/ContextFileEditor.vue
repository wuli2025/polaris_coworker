<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { convApi } from '../../../tauri';
import { useContextFileEditor } from '../useContextFileEditor';

const projects = ref<{ id: string; name: string }[]>([]);
const { projectId, createFile, selection, content, busy, error, saved, open, save, switchProject } = useContextFileEditor();
async function changeProject(event: Event) {
  const select = event.target as HTMLSelectElement;
  const next = select.value;
  select.value = projectId.value;
  await switchProject(next);
}
onMounted(async () => {
  try {
    projects.value = await convApi.listProjects();
    projectId.value = projects.value[0]?.id ?? '';
    if (projectId.value) await open();
  } catch (e) { error.value = String(e); }
});
</script>
<template>
  <section class="context-editor">
    <p>Pi 按项目实际工作目录优先读取 AGENTS.md，否则读取 CLAUDE.md；保存始终写回本次选中的文件。</p>
    <label>项目 <select :value="projectId" :disabled="busy" @change="changeProject">
      <option v-for="project in projects" :key="project.id" :value="project.id">{{ project.name }}</option>
    </select></label>
    <label>两者都不存在时创建 <select v-model="createFile" :disabled="busy">
      <option>AGENTS.md</option><option>CLAUDE.md</option>
    </select></label>
    <button :disabled="busy || !projectId" @click="open">读取 / 重新扫描</button>
    <template v-if="selection">
      <p>已选择：<strong>{{ selection.fileName }}</strong> · 相对路径：{{ selection.relativePath }}</p>
      <p>实际工作目录：{{ selection.root }}</p>
      <p>AGENTS.md：{{ selection.agentsExists ? '存在' : '不存在' }}；CLAUDE.md：{{ selection.claudeExists ? '存在' : '不存在' }}</p>
      <p v-if="!selection.exists">文件不存在，保存时创建 {{ selection.fileName }}。</p>
      <textarea v-model="content" :disabled="busy" :aria-label="selection.fileName" spellcheck="false" />
      <button :disabled="busy" @click="save">{{ busy ? '处理中…' : `保存 ${selection.fileName}` }}</button>
    </template>
    <p v-if="saved" role="status">已保存；新会话读取更新后的上下文。</p>
    <p v-if="error" role="alert">{{ error }}</p>
    <p v-if="!projects.length && !error">暂无项目。</p>
  </section>
</template>
<style scoped>
.context-editor { padding: 20px; overflow: auto; }
label { display: inline-block; margin: 8px; }
textarea { display: block; width: 100%; min-height: 320px; margin: 12px 0; font-family: monospace; }
[role=alert] { color: var(--danger, #dc2626); }
</style>
