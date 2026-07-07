<script setup lang="ts">
/**
 * 团队项目主页(GitHub repo 首页式):概览 / 任务 / 讨论 / 成员 四 tab。
 * - 概览:六态统计 + 动态时间线(activity)+ 成员条
 * - 任务:直嵌 TaskBoard(无 props,纯读 collab store)
 * - 讨论:绑定的本地 conv 项目下的对话(首次「开新讨论」自动建同名项目并绑定,git clone 式)
 */
import { computed, onMounted, ref } from "vue";
import {
  ArrowLeft,
  CircleDot,
  FolderGit2,
  History,
  Kanban,
  LoaderCircle,
  MessageSquarePlus,
  MessagesSquare,
  Users,
} from "@lucide/vue";
import { useAppStore } from "../../stores/app";
import { useCollabStore } from "./stores/collab";
import { fmtTime } from "./api";
import type { Conversation } from "../../tauri";
import TaskBoard from "./TaskBoard.vue";
import { toast } from "../../composables/useToast";

const app = useAppStore();
const collab = useCollabStore();

type Tab = "overview" | "tasks" | "talks" | "members";
const tab = ref<Tab>("overview");
const TABS: { key: Tab; label: string }[] = [
  { key: "overview", label: "概览" },
  { key: "tasks", label: "任务" },
  { key: "talks", label: "讨论" },
  { key: "members", label: "成员" },
];

const proj = computed(() => collab.currentProject);
const teamName = computed(
  () => collab.teams.find((t) => t.id === proj.value?.team_id)?.name ?? ""
);

// 六态统计(直接从已加载的看板数据聚合)
const STAT_LABELS: { key: string; label: string }[] = [
  { key: "pending", label: "待认领" },
  { key: "in_progress", label: "进行中" },
  { key: "review", label: "待验收" },
  { key: "merged", label: "已合并" },
];
const stat = computed(() => {
  const by: Record<string, number> = {};
  for (const t of collab.tasks) by[t.state] = (by[t.state] ?? 0) + 1;
  return by;
});

// ── 讨论(绑定的本地项目) ──
const bound = computed(() =>
  proj.value ? app.projectByCollabId(proj.value.id) : undefined
);
const talks = computed<Conversation[]>(() =>
  bound.value ? (app.conversationsByProject[bound.value.id] ?? []) : []
);

/** 确保绑定的本地项目存在(首次自动建同名项目并绑定) */
async function ensureBound(): Promise<string | null> {
  if (!proj.value) return null;
  if (bound.value) return bound.value.id;
  const p = await app.createProject(proj.value.name);
  await app.bindProjectToCollab(p.id, proj.value.id, collab.base);
  return p.id;
}

const talkBusy = ref(false);
async function newTalk() {
  talkBusy.value = true;
  try {
    const pid = await ensureBound();
    if (!pid) return;
    await app.createConversation(pid); // 内部 setView("chat")
  } catch (e) {
    toast.error((e as Error).message);
  } finally {
    talkBusy.value = false;
  }
}
function openTalk(c: Conversation) {
  app.selectConversation(c);
}

function backToCollab() {
  app.setView("collab");
}

function relTime(ms: number): string {
  const d = Date.now() - ms;
  if (d < 60_000) return "刚刚";
  if (d < 3_600_000) return `${Math.floor(d / 60_000)} 分钟前`;
  if (d < 86_400_000) return `${Math.floor(d / 3_600_000)} 小时前`;
  return `${Math.floor(d / 86_400_000)} 天前`;
}

const STATE_LABEL: Record<string, string> = {
  pending: "待认领",
  in_progress: "进行中",
  review: "待验收",
  merged: "已合并",
  archived: "已归档",
  cancelled: "已取消",
};

onMounted(() => {
  void collab.init();
  // 直接冷启进本页(如侧栏点击后刷新):init 里 afterAuth 会拉项目;动态单独补一发
  void collab.refreshActivity();
  // 绑定项目的对话列表需要 conv 数据就位
  void app.refreshProjects();
});
</script>

<template>
  <div class="phome">
    <div v-if="!collab.authed || !proj" class="ph-empty">
      <p>{{ collab.authed ? "没有选中的团队项目" : "请先在「协作」里登录" }}</p>
      <button class="btn ghost" @click="backToCollab"><ArrowLeft :size="13" /> 去协作中心</button>
    </div>

    <template v-else>
      <!-- 头部:名称 + 团队 + repo + 徽章 -->
      <header class="ph-head">
        <div class="ph-title">
          <FolderGit2 :size="18" :stroke-width="1.7" />
          <span v-if="teamName" class="ph-team">{{ teamName }} /</span>
          <h1>{{ proj.name }}</h1>
          <span v-if="(proj.open_count ?? 0) > 0" class="ph-badge">{{ proj.open_count }} 进行中</span>
          <span v-if="(proj.review_count ?? 0) > 0" class="ph-badge review">{{ proj.review_count }} 待验收</span>
        </div>
        <p v-if="proj.repo" class="ph-repo mono">{{ proj.repo }}</p>
      </header>

      <!-- tab 条 -->
      <nav class="ph-tabs">
        <button
          v-for="t in TABS"
          :key="t.key"
          class="ph-tab"
          :class="{ active: tab === t.key }"
          @click="tab = t.key"
        >
          <Kanban v-if="t.key === 'tasks'" :size="13" />
          <MessagesSquare v-else-if="t.key === 'talks'" :size="13" />
          <Users v-else-if="t.key === 'members'" :size="13" />
          <CircleDot v-else :size="13" />
          {{ t.label }}
          <span v-if="t.key === 'talks' && talks.length" class="ph-count">{{ talks.length }}</span>
        </button>
      </nav>

      <!-- ── 概览 ── -->
      <div v-if="tab === 'overview'" class="ph-body">
        <div class="ov-stats">
          <div v-for="s in STAT_LABELS" :key="s.key" class="ov-tile">
            <div class="ov-num">{{ stat[s.key] ?? 0 }}</div>
            <div class="ov-lb">{{ s.label }}</div>
          </div>
        </div>

        <section class="ov-sec">
          <h3><History :size="14" /> 动态</h3>
          <div v-if="!collab.activity.length" class="ph-dim">还没有动态——建一张任务卡开工吧</div>
          <ul v-else class="feed">
            <li v-for="(a, i) in collab.activity" :key="i" class="feed-item">
              <span class="feed-ic" :class="a.kind">
                <History v-if="a.kind === 'review'" :size="12" />
                <CircleDot v-else :size="12" />
              </span>
              <span class="feed-text">
                <b>{{ a.actor || "—" }}</b>
                <template v-if="a.kind === 'review'">
                  验收了 <em>#{{ a.task_id }} {{ a.title }}</em>({{ a.detail.replace("pass", "通过").replace("reject", "打回") }})
                </template>
                <template v-else>
                  <em>#{{ a.task_id }} {{ a.title }}</em> → {{ STATE_LABEL[a.detail] ?? a.detail }}
                </template>
              </span>
              <span class="feed-time">{{ fmtTime(a.at) }}</span>
            </li>
          </ul>
        </section>

        <section class="ov-sec">
          <h3><Users :size="14" /> 成员</h3>
          <div class="ov-members">
            <span v-for="m in collab.members" :key="m.user_id" class="ov-chip" :title="'@' + m.username">
              {{ (m.display_name || m.username).slice(0, 1) }}
              <i>{{ m.display_name || m.username }}</i>
            </span>
            <span v-if="!collab.members.length" class="ph-dim">暂无成员</span>
          </div>
        </section>
      </div>

      <!-- ── 任务(直嵌看板) ── -->
      <div v-else-if="tab === 'tasks'" class="ph-body board">
        <TaskBoard />
      </div>

      <!-- ── 讨论 ── -->
      <div v-else-if="tab === 'talks'" class="ph-body">
        <div class="talks-head">
          <button class="btn solid" :disabled="talkBusy" @click="newTalk">
            <LoaderCircle v-if="talkBusy" :size="13" class="spin" />
            <MessageSquarePlus v-else :size="13" />
            开新讨论
          </button>
          <span v-if="bound" class="ph-dim">对话存放在本地项目「{{ bound.name }}」下,侧栏可直达</span>
        </div>
        <div v-if="!talks.length" class="ph-dim" style="margin-top: 12px">
          还没有讨论 —— 项目里的事,开个对话聊起来
        </div>
        <ul v-else class="talk-list">
          <li v-for="c in talks" :key="c.id" class="talk-item" @click="openTalk(c)">
            <MessagesSquare :size="14" :stroke-width="1.7" />
            <span class="talk-title">{{ c.title || "未命名对话" }}</span>
            <span class="talk-time">{{ relTime(c.updatedAt) }}</span>
          </li>
        </ul>
      </div>

      <!-- ── 成员 ── -->
      <div v-else class="ph-body">
        <table class="ph-tbl">
          <thead>
            <tr><th>成员</th><th>用户名</th><th>角色</th></tr>
          </thead>
          <tbody>
            <tr v-for="m in collab.members" :key="m.user_id">
              <td>{{ m.display_name || m.username }}</td>
              <td class="mono">@{{ m.username }}</td>
              <td>{{ m.role }}</td>
            </tr>
          </tbody>
        </table>
        <div v-if="!collab.members.length" class="ph-dim">暂无成员——去协作中心拉人</div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.phome { flex: 1; display: flex; flex-direction: column; overflow: hidden; background: var(--bg); }
.ph-empty {
  flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
  gap: 12px; color: var(--muted); font-size: 13px;
}

.ph-head { padding: 18px 22px 0; }
.ph-title { display: flex; align-items: center; gap: 8px; color: var(--ink); }
.ph-title h1 { margin: 0; font-size: 17px; font-weight: 700; }
.ph-team { font-size: 14px; color: var(--muted); }
.ph-badge {
  font-size: 11px; font-weight: 600; padding: 2px 8px; border-radius: 999px;
  color: var(--primary, var(--ink)); background: var(--primary-soft, var(--selection-bg));
}
.ph-badge.review { color: #b8860b; background: color-mix(in srgb, #b8860b 12%, transparent); }
.ph-repo { margin: 6px 0 0 26px; font-size: 11.5px; color: var(--muted); }
.mono { font-family: var(--mono); }

.ph-tabs {
  display: flex; gap: 2px; padding: 10px 22px 0; border-bottom: 1px solid var(--border-soft);
}
.ph-tab {
  display: inline-flex; align-items: center; gap: 5px;
  border: none; background: none; cursor: pointer;
  font-size: 12.5px; color: var(--muted); padding: 8px 12px;
  border-bottom: 2px solid transparent; margin-bottom: -1px;
}
.ph-tab.active { color: var(--ink); font-weight: 600; border-bottom-color: var(--ink); }
.ph-count {
  font-size: 10px; padding: 0 6px; border-radius: 999px;
  background: var(--selection-bg); color: var(--muted);
}

.ph-body { flex: 1; overflow-y: auto; padding: 16px 22px; }
.ph-body.board { padding: 0; display: flex; flex-direction: column; overflow: hidden; }
.ph-dim { font-size: 12px; color: var(--dim); font-style: italic; }

.ov-stats { display: flex; gap: 10px; flex-wrap: wrap; }
.ov-tile {
  flex: 1; min-width: 110px; border: 1px solid var(--border-soft); border-radius: 10px;
  background: var(--panel); padding: 12px 14px; text-align: center;
}
.ov-num { font-size: 22px; font-weight: 700; color: var(--ink); }
.ov-lb { font-size: 11.5px; color: var(--muted); margin-top: 2px; }

.ov-sec { margin-top: 18px; }
.ov-sec h3 {
  display: flex; align-items: center; gap: 6px; margin: 0 0 8px;
  font-size: 13px; font-weight: 600; color: var(--ink);
}
.feed { list-style: none; margin: 0; padding: 0; }
.feed-item {
  display: flex; align-items: baseline; gap: 8px; padding: 7px 0;
  border-bottom: 1px dashed var(--border-soft); font-size: 12.5px; color: var(--text);
}
.feed-ic { color: var(--muted); align-self: center; display: inline-flex; }
.feed-ic.review { color: #b8860b; }
.feed-text { flex: 1; min-width: 0; }
.feed-text em { font-style: normal; color: var(--ink); }
.feed-time { font-size: 11px; color: var(--dim); white-space: nowrap; }

.ov-members { display: flex; flex-wrap: wrap; gap: 8px; }
.ov-chip {
  display: inline-flex; align-items: center; gap: 6px;
  font-size: 12px; color: var(--text); background: var(--panel);
  border: 1px solid var(--border-soft); border-radius: 999px; padding: 3px 10px 3px 4px;
}
.ov-chip::first-letter { text-transform: uppercase; }
.ov-chip i { font-style: normal; }

.talks-head { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
.talk-list { list-style: none; margin: 12px 0 0; padding: 0; }
.talk-item {
  display: flex; align-items: center; gap: 9px; padding: 9px 10px;
  border-radius: 8px; cursor: pointer; color: var(--text); font-size: 13px;
}
.talk-item:hover { background: var(--selection-bg); }
.talk-title { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.talk-time { font-size: 11px; color: var(--dim); }

.ph-tbl { width: 100%; border-collapse: collapse; font-size: 12.5px; }
.ph-tbl th {
  text-align: left; font-size: 11px; color: var(--muted); font-weight: 600;
  padding: 6px 8px; border-bottom: 1px solid var(--border-soft);
}
.ph-tbl td { padding: 8px; border-bottom: 1px solid var(--border-soft); color: var(--text); }

.btn {
  display: inline-flex; align-items: center; gap: 6px; cursor: pointer;
  border-radius: 8px; font-size: 12.5px; padding: 7px 12px;
  border: 1px solid var(--border); background: var(--panel); color: var(--ink);
}
.btn.solid { background: var(--ink); color: var(--bg); border-color: var(--ink); }
.btn.ghost { background: none; }
.btn:disabled { opacity: 0.6; cursor: default; }
.spin { animation: spin 0.9s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
