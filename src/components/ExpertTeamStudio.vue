<script setup lang="ts">
/**
 * ExpertTeamStudio — 专家团工作台（统一工作区 + 可视化）
 *
 * 显示当前项目激活的专家团队:
 * - 从项目 personaId 读取当前专家
 * - 团队board: 头像 + 名字 + 角色 + 为什么选你
 * - 战略师作为编排者在顶部，其余专家按补维排列
 */
import { ref, computed, onMounted, watch } from "vue";
import { expert, convApi, type ExpertCard } from "../tauri";

const props = defineProps<{
  projectId: string;
}>();

const activeExpertId = ref<string | null>(null);
const activeExpert = ref<ExpertCard | null>(null);
const avatarCache = ref<Map<string, string>>(new Map());
const loading = ref(true);

onMounted(async () => {
  await loadActiveExpert();
});

async function loadActiveExpert() {
  loading.value = true;
  try {
    const projects = await convApi.listProjects();
    const proj = projects.find((p) => p.id === props.projectId);
    if (proj?.personaId) {
      const card = await expert.get(proj.personaId);
      activeExpert.value = card;
    } else {
      activeExpert.value = null;
    }
  } catch (e) {
    console.error("加载当前专家失败", e);
  } finally {
    loading.value = false;
  }
}

watch(() => props.projectId, loadActiveExpert);

const teamMembers = computed(() => {
  if (!activeExpert.value) return [];
  const exp = activeExpert.value;
  return [
    {
      id: exp.id,
      name: exp.name,
      icon: exp.icon,
      role: exp.role,
      description: exp.description,
      hitSignals: exp.triggerSignals.slice(0, 3),
      complements: exp.complements,
      costTier: exp.costTier,
    },
  ];
});

const tierLabel: Record<number, string> = { 1: "便宜路由", 2: "中档专业", 3: "深度推理" };
const tierColor: Record<number, string> = { 1: "#5fd39a", 2: "#e6c984", 3: "#c79cff" };
const tierLeftColor: Record<number, string> = { 1: "#5fd39a", 2: "#d4b06a", 3: "#c79cff" };

async function loadAvatar(id: string, icon: string): Promise<string> {
  if (avatarCache.value.has(id)) return avatarCache.value.get(id)!;
  try {
    const dataUrl = await expert.getAvatar(id);
    if (dataUrl) {
      avatarCache.value.set(id, dataUrl);
      return dataUrl;
    }
  } catch {
    /* ignore */
  }
  const colors = ["#d4b06a", "#b07bff", "#5fd39a", "#6ea8ff", "#e6c984", "#c79cff"];
  const color = colors[icon.charCodeAt(0) % colors.length];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="72" height="72" viewBox="0 0 72 72"><defs><linearGradient id="g" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" stop-color="${color}"/><stop offset="100%" stop-color="#6a6a7a"/></linearGradient></defs><circle cx="36" cy="36" r="36" fill="url(#g)"/><text x="50%" y="54%" font-size="26" text-anchor="middle" dominant-baseline="middle">${icon}</text></svg>`;
  const fallback = `data:image/svg+xml,${encodeURIComponent(svg)}`;
  avatarCache.value.set(id, fallback);
  return fallback;
}

function avatarUrl(id: string, icon: string): string {
  if (!avatarCache.value.has(id)) {
    loadAvatar(id, icon);
  }
  return avatarCache.value.get(id) ?? "";
}
</script>

<template>
  <div class="studio">
    <div class="studio-head">
      <span class="studio-title">🧭 专家团工作台</span>
      <span v-if="activeExpert" class="studio-badge">
        {{ activeExpert.icon }} {{ activeExpert.name }}
      </span>
      <span v-else class="studio-empty-hint">未入驻专家</span>
    </div>

    <div v-if="loading" class="studio-loading">
      <span class="loading-dot" />
      <span class="loading-dot" />
      <span class="loading-dot" />
    </div>

    <div v-else-if="!activeExpert" class="studio-empty">
      <div class="empty-icon">🧭</div>
      <p class="empty-title">尚未入驻专家</p>
      <p class="empty-sub">在「模式」中选择「单专家」或「专家团」即可邀请专家入驻</p>
    </div>

    <div v-else class="team-board">
      <div
        v-for="(member, idx) in teamMembers"
        :key="member.id"
        class="member-card"
        :style="{ '--tier-left': tierLeftColor[member.costTier] }"
      >
        <div class="member-avatar-wrap">
          <img
            v-if="avatarUrl(member.id, member.icon)"
            :src="avatarUrl(member.id, member.icon)"
            :alt="member.name"
            class="member-avatar"
          />
          <div v-else class="member-avatar-placeholder">{{ member.icon }}</div>
          <div v-if="idx === 0" class="orchestrator-badge">🧭</div>
        </div>

        <div class="member-info">
          <div class="member-name-row">
            <span class="member-name">{{ member.icon }} {{ member.name }}</span>
            <span
              class="member-tier"
              :style="{ color: tierColor[member.costTier], borderColor: tierColor[member.costTier] + '55' }"
            >{{ tierLabel[member.costTier] }}</span>
          </div>
          <div class="member-role">{{ member.role }}</div>

          <div class="member-reason">
            <div class="reason-label">为什么选你</div>
            <div class="reason-body">
              <span
                v-for="signal in member.hitSignals"
                :key="signal"
                class="reason-chip"
              >{{ signal }}</span>
              <span v-if="member.complements" class="reason-comp">补维: {{ member.complements }}</span>
            </div>
          </div>

          <p class="member-desc">{{ member.description }}</p>
        </div>
      </div>

      <div class="orchestrate-note">
        <div class="note-icon">💡</div>
        <p>战略师领衔，按需召集。如需更多专家参与，可切换至「专家团」模式并描述任务目标。</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.studio {
  padding: 14px 16px;
  background: var(--panel, rgba(24, 24, 34, 0.66));
  border-radius: 12px;
  border: 1px solid var(--line, rgba(255, 255, 255, 0.08));
  min-height: 160px;
}

.studio-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 14px;
}
.studio-title {
  font-size: 14px;
  font-weight: 700;
  color: var(--ink, #e9e8ee);
}
.studio-badge {
  font-size: 12px;
  background: rgba(212, 176, 106, 0.15);
  border: 1px solid rgba(212, 176, 106, 0.3);
  color: var(--gold, #d4b06a);
  padding: 2px 10px;
  border-radius: 20px;
}
.studio-empty-hint {
  font-size: 12px;
  color: var(--faint, #6f6e7e);
}

.studio-loading {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 6px;
  padding: 32px;
}
.loading-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--gold, #d4b06a);
  animation: dotPulse 1.2s ease-in-out infinite;
}
.loading-dot:nth-child(2) { animation-delay: 0.2s; }
.loading-dot:nth-child(3) { animation-delay: 0.4s; }
@keyframes dotPulse {
  0%, 80%, 100% { opacity: 0.3; transform: scale(0.8); }
  40% { opacity: 1; transform: scale(1.1); }
}

.studio-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 20px;
  text-align: center;
}
.empty-icon { font-size: 36px; margin-bottom: 8px; opacity: 0.5; }
.empty-title { font-size: 14px; font-weight: 600; color: var(--ink); margin-bottom: 4px; }
.empty-sub { font-size: 12px; color: var(--dim, #a7a6b4); line-height: 1.5; max-width: 260px; }

.team-board { display: flex; flex-direction: column; gap: 12px; }

.member-card {
  display: flex;
  gap: 14px;
  padding: 12px 14px;
  border-radius: 12px;
  border: 1px solid var(--line);
  border-left: 3px solid var(--tier-left, var(--line));
  background: var(--panel2, rgba(32, 32, 46, 0.5));
  position: relative;
}

.member-avatar-wrap {
  position: relative;
  flex: 0 0 72px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.member-avatar {
  width: 72px;
  height: 72px;
  border-radius: 50%;
  object-fit: cover;
  border: 2px solid rgba(255, 255, 255, 0.1);
}
.member-avatar-placeholder {
  width: 72px;
  height: 72px;
  border-radius: 50%;
  background: linear-gradient(135deg, #d4b06a44, #b07bff44);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 32px;
}
.orchestrator-badge {
  position: absolute;
  bottom: -2px;
  right: -2px;
  font-size: 18px;
  background: var(--panel, rgba(24,24,34,.8));
  border-radius: 50%;
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid rgba(212,176,106,.4);
}

.member-info { flex: 1; display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.member-name-row { display: flex; align-items: center; gap: 8px; }
.member-name { font-size: 15px; font-weight: 700; color: var(--ink); }
.member-tier {
  font-size: 10px;
  padding: 1px 7px;
  border-radius: 4px;
  border: 1px solid;
}
.member-role { font-size: 12px; color: var(--dim); }

.member-reason { margin-top: 6px; }
.reason-label { font-size: 10px; color: var(--faint); margin-bottom: 4px; }
.reason-body { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
.reason-chip {
  font-size: 10px;
  background: rgba(110, 168, 255, 0.15);
  border: 1px solid rgba(110, 168, 255, 0.3);
  color: #6ea8ff;
  padding: 1px 7px;
  border-radius: 4px;
}
.reason-comp { font-size: 11px; color: var(--dim); }

.member-desc {
  font-size: 12px;
  color: var(--dim);
  margin: 4px 0 0;
  line-height: 1.5;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.orchestrate-note {
  display: flex;
  gap: 8px;
  padding: 10px 12px;
  border-radius: 8px;
  background: rgba(212, 176, 106, 0.06);
  border: 1px solid rgba(212, 176, 106, 0.15);
}
.note-icon { font-size: 14px; flex: 0 0 20px; }
.orchestrate-note p {
  font-size: 12px;
  color: var(--dim);
  margin: 0;
  line-height: 1.5;
}
</style>