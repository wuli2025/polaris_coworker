<script setup lang="ts">
import { onMounted, ref } from "vue";
import { collab, type CollabProject, type TaskCard } from "../lib/net";
import { toastErr, toast } from "../lib/toast";

const projects = ref<CollabProject[]>([]);
const active = ref<CollabProject | null>(null);
const tasks = ref<TaskCard[]>([]);
const loading = ref(false);

const STATE_LABEL: Record<string, string> = {
  pending: "待认领",
  in_progress: "进行中",
  review: "待验收",
  merged: "已合并",
  archived: "已归档",
  cancelled: "已取消",
};

async function loadProjects() {
  loading.value = true;
  try {
    projects.value = await collab.listProjects();
  } catch (e) {
    toastErr(e);
  } finally {
    loading.value = false;
  }
}
async function openProject(p: CollabProject) {
  active.value = p;
  try {
    tasks.value = await collab.listTasks(p.id);
  } catch (e) {
    toastErr(e);
  }
}
async function claim(t: TaskCard) {
  try {
    await collab.claimTask(t.id);
    toast("已认领", "ok");
    if (active.value) openProject(active.value);
  } catch (e) {
    toastErr(e);
  }
}
async function submit(t: TaskCard) {
  try {
    await collab.submitTask(t.id);
    toast("已送验", "ok");
    if (active.value) openProject(active.value);
  } catch (e) {
    toastErr(e);
  }
}

onMounted(loadProjects);
</script>

<template>
  <div class="proj">
    <header class="bar">
      <button v-if="active" class="icon" @click="active = null">‹</button>
      <div class="title">{{ active ? active.name : "项目" }}</div>
      <span style="width: 34px"></span>
    </header>

    <div class="list">
      <template v-if="!active">
        <p v-if="!projects.length && !loading" class="empty muted">
          还没有项目。在桌面端或 Web 端创建后即可在此认领任务。
        </p>
        <button
          v-for="p in projects"
          :key="p.id"
          class="prow"
          @click="openProject(p)"
        >
          <span class="meta">
            <span class="nm">{{ p.name }}</span>
            <span class="sub faint">{{ p.repo }}</span>
          </span>
          <span class="badges">
            <span v-if="p.open_count" class="badge">{{ p.open_count }} 进行</span>
            <span v-if="p.review_count" class="badge rev">{{ p.review_count }} 待验</span>
          </span>
        </button>
      </template>

      <template v-else>
        <p v-if="!tasks.length" class="empty muted">该项目暂无任务卡</p>
        <div v-for="t in tasks" :key="t.id" class="task">
          <div class="thead">
            <span class="tstate" :class="t.state">{{ STATE_LABEL[t.state] || t.state }}</span>
            <span class="ttitle">{{ t.title }}</span>
          </div>
          <p v-if="t.body" class="tbody faint">{{ t.body }}</p>
          <div class="tacts">
            <button v-if="t.state === 'pending'" class="btn sm" @click="claim(t)">认领</button>
            <button v-if="t.state === 'in_progress'" class="btn sm ghost" @click="submit(t)">
              送验
            </button>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.proj {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: calc(10px + var(--safe-top)) 12px 10px;
  border-bottom: 1px solid var(--line);
}
.title {
  font-weight: 600;
}
.icon {
  font-size: 26px;
  color: var(--text-dim);
  width: 34px;
}
.list {
  flex: 1;
  overflow-y: auto;
  padding: 12px 14px 20px;
}
.empty {
  text-align: center;
  padding: 40px 20px;
}
.prow {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 14px;
  background: var(--bg-elev);
  border: 1px solid var(--line);
  border-radius: 12px;
  margin-bottom: 10px;
  color: var(--text);
}
.meta {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  min-width: 0;
}
.nm {
  font-weight: 600;
}
.sub {
  font-size: 12px;
}
.badges {
  display: flex;
  gap: 6px;
}
.badge {
  font-size: 11px;
  padding: 3px 8px;
  border-radius: 8px;
  background: var(--accent-soft);
  color: var(--accent);
}
.badge.rev {
  background: #4a3a10;
  color: #f5c451;
}
.task {
  background: var(--bg-elev);
  border: 1px solid var(--line);
  border-radius: 12px;
  padding: 12px 14px;
  margin-bottom: 10px;
}
.thead {
  display: flex;
  align-items: center;
  gap: 8px;
}
.ttitle {
  font-weight: 600;
}
.tstate {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 7px;
  background: var(--bg-elev2);
  color: var(--text-dim);
}
.tstate.in_progress {
  background: var(--accent-soft);
  color: var(--accent);
}
.tstate.review {
  background: #4a3a10;
  color: #f5c451;
}
.tbody {
  margin: 8px 0 0;
  font-size: 13px;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.tacts {
  margin-top: 10px;
  display: flex;
  gap: 8px;
}
.btn.sm {
  padding: 7px 16px;
  font-size: 13px;
  border-radius: 9px;
}
</style>
