<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../../../stores/app";
import { isTauri } from "../../../tauri";
import { needsSandboxMigration, projectDirectoryHint, startProjectSandbox } from "../project";

const props = defineProps<{ disabled?: boolean; notes?: string[] }>();
const emit = defineEmits<{ log: [message: string]; changed: [] }>();
const app = useAppStore();
const project = computed(() => app.projects.find(p => p.id === app.currentProjectId));
const directory = ref("");
const pathHint = computed(() => projectDirectoryHint(directory.value));
const busy = ref(false);
const error = ref("");
watch(() => [project.value?.id, project.value?.workDir], () => {
  directory.value = project.value?.workDir ?? "";
  error.value = "";
}, { immediate: true });
const migration = computed(() => needsSandboxMigration([...(props.notes ?? []), error.value]));

async function pick() {
  try {
    const selected = await open({ directory: true, multiple: false, title: "选择沙箱项目工作目录" });
    if (typeof selected === "string") directory.value = selected;
  } catch (cause) { error.value = String(cause); }
}
async function start(rebuild = false) {
  busy.value = true;
  error.value = "";
  const projectId = project.value?.id;
  const selected = directory.value.trim();
  try {
    const message = await startProjectSandbox(selected, rebuild);
    emit("log", message);
    // Bind the confirmed directory so the next conversation turn uses the same mount.
    if (projectId) await app.setProjectWorkDir(projectId, selected);
  } catch (cause) {
    error.value = String(cause);
    emit("log", error.value);
  } finally {
    busy.value = false;
    emit("changed");
  }
}
</script>

<template>
  <section class="sandbox-project">
    <label for="sandbox-project-dir">项目工作目录{{ project ? ` · ${project.name}` : "" }}</label>
    <p>默认使用当前会话所属项目的工作目录。确认后将作为该项目的工作目录；知识库只读挂载。</p>
    <div class="project-actions">
      <input id="sandbox-project-dir" v-model="directory" :aria-invalid="!!pathHint" aria-describedby="sandbox-project-path-hint" :disabled="busy || disabled" placeholder="输入项目目录的绝对路径" />
      <button v-if="isTauri" :disabled="busy || disabled" @click="pick">选择文件夹</button>
      <button :disabled="busy || disabled || !directory.trim()" @click="start()">确认目录并启动</button>
    </div>
    <p id="sandbox-project-path-hint" aria-live="polite">{{ pathHint || "路径格式符合要求；目录是否可用以后端校验为准。" }}</p>
    <div v-if="migration" role="alert">
      <p>当前容器需要迁移。重建会停止旧容器并改名保留，再按所选项目创建安全容器。</p>
      <button :disabled="busy || disabled || !directory.trim()" @click="start(true)">保留旧容器并一键重建</button>
    </div>
    <p v-if="error" role="alert">{{ error }}</p>
  </section>
</template>

<style scoped>
.sandbox-project { margin-bottom: 16px; padding: 14px; border: 1px solid var(--border); border-radius: 4px; }
p { font-size: 12px; color: var(--text-2); overflow-wrap: anywhere; }
.project-actions { display: flex; gap: 8px; flex-wrap: wrap; }
input { flex: 1; min-width: 200px; padding: 7px; background: var(--bg); color: var(--text); border: 1px solid var(--border); }
button { padding: 7px 12px; background: var(--panel); color: var(--text); border: 1px solid var(--border); border-radius: 4px; }
button:disabled { opacity: 0.5; }
</style>
