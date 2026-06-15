<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import {
  expert,
  type ExpertCard,
  type ExpertGroup,
  type ExpertMatch,
} from "../tauri";

// ── 专业 SVG 图标（比 emoji 更精致）─────────────────────────────
const IconAgent = `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="10" rx="2"/><circle cx="12" cy="5" r="3"/><path d="M12 8v3"/><path d="M8 15h0M16 15h0"/></svg>`;
const IconExpert = `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="4"/><path d="M4 20c0-4 3.6-7 8-7s8 3 8 7"/><circle cx="19" cy="5" r="2.5"/><path d="M19 7.5v-1l1.5-1.5"/></svg>`;
const IconTeam = `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="7" r="3"/><circle cx="17" cy="7" r="3"/><path d="M3 20c0-4 2.7-7 6-7"/><path d="M15 15c0 3.3-2.7 6-6 6s-6-2.7-6-6"/><path d="M21 20c0-2.7-1.3-5-3-6"/><path d="M21 14c0 2.7-1.3 5-3 6"/></svg>`;
const IconSpark = `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2L9 9l-7 1 5 5-1.5 7L12 18l6.5 4L17 15l5-5-7-1z"/></svg>`;

// 4种模式 — WorkBuddy 风格
type ChatMode = "single-agent" | "single-expert" | "expert-team" | "auto-match";
const modes: { id: ChatMode; label: string; icon: string; desc: string }[] = [
  { id: "single-agent", label: "单Agent", icon: IconAgent, desc: "无专家加成，最便宜" },
  { id: "single-expert", label: "选专家", icon: IconExpert, desc: "从花名册选一个专家" },
  { id: "expert-team", label: "选团队", icon: IconTeam, desc: "选一支专家团入驻" },
  { id: "auto-match", label: "智能匹配", icon: IconSpark, desc: "描述需求，自动路由最合适专家" },
];

const currentMode = ref<ChatMode>("auto-match");
const groups = ref<ExpertGroup[]>([]);
const allExperts = ref<ExpertCard[]>([]);
const filteredExperts = ref<ExpertCard[]>([]);
const selectedGroup = ref<string | null>(null);
const autoMatchResults = ref<ExpertMatch[]>([]);
const searchQuery = ref("");
const loading = ref(false);

// 专家头像缓存
const avatarCache = ref<Map<string, string>>(new Map());
const avatarLoading = ref<Set<string>>(new Set());

const modeDesc = computed(() => modes.find((m) => m.id === currentMode.value)?.desc ?? "");

// 团队预设（WorkBuddy 风格：每支团队是一个编排型 CLAUDE.md）
const teams = [
  {
    id: "team-general",
    name: "全能专家团",
    icon: "🧭",
    desc: "战略师领衔，按需召集，最常用",
    tags: ["战略", "编排", "全栈"],
  },
  {
    id: "team-creative",
    name: "创作专家团",
    icon: "🎨",
    desc: "PPT / 网页 / 自媒体 / 视频成品",
    tags: ["UI设计", "叙事", "交付"],
  },
  {
    id: "team-research",
    name: "研究专家团",
    icon: "🔬",
    desc: "调研 / 尽调 / 选型，多源校验",
    tags: ["研究", "分析", "溯源"],
  },
];

onMounted(async () => {
  try {
    const [g, exps] = await Promise.all([expert.groups(), expert.list()]);
    groups.value = g;
    allExperts.value = exps;
    filteredExperts.value = exps;
  } catch (e) {
    console.error("加载专家库失败", e);
  }
});

function selectMode(mode: ChatMode) {
  currentMode.value = mode;
  emit("mode-change", mode);
  if (mode === "single-expert") {
    filteredExperts.value = selectedGroup.value
      ? allExperts.value.filter((e) => e.group === selectedGroup.value)
      : allExperts.value;
  } else if (mode === "auto-match") {
    runAutoMatch(searchQuery.value);
  }
}

function selectGroup(g: string | null) {
  selectedGroup.value = g;
  filteredExperts.value = g
    ? allExperts.value.filter((e) => e.group === g)
    : allExperts.value;
}

function selectExpert(exp: ExpertCard) {
  emit("select", exp.id);
}

function selectTeam(teamId: string) {
  emit("select", teamId);
}

async function runAutoMatch(query: string) {
  if (!query.trim()) {
    autoMatchResults.value = [];
    return;
  }
  loading.value = true;
  try {
    autoMatchResults.value = await expert.matchAuto(query);
  } catch (e) {
    console.error("自动匹配失败", e);
  } finally {
    loading.value = false;
  }
}

async function loadAvatar(id: string, icon: string): Promise<string> {
  if (avatarCache.value.has(id)) return avatarCache.value.get(id)!;
  if (avatarLoading.value.has(id)) return "";
  avatarLoading.value.add(id);
  try {
    const dataUrl = await expert.getAvatar(id);
    if (dataUrl) {
      avatarCache.value.set(id, dataUrl);
      return dataUrl;
    }
  } catch (e) {
    /* ignore */
  } finally {
    avatarLoading.value.delete(id);
  }
  const colors = ["#d4b06a", "#b07bff", "#5fd39a", "#6ea8ff", "#e6c984", "#c79cff"];
  const color = colors[icon.charCodeAt(0) % colors.length];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="56" height="56" viewBox="0 0 56 56"><defs><linearGradient id="g" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" stop-color="${color}"/><stop offset="100%" stop-color="#6a6a7a"/></linearGradient></defs><circle cx="28" cy="28" r="28" fill="url(#g)"/><text x="50%" y="54%" font-size="20" text-anchor="middle" dominant-baseline="middle">${icon}</text></svg>`;
  const fallback = `data:image/svg+xml,${encodeURIComponent(svg)}`;
  avatarCache.value.set(id, fallback);
  return fallback;
}

function avatarUrl(id: string, icon: string): string {
  if (!avatarCache.value.has(id) && !avatarLoading.value.has(id)) {
    loadAvatar(id, icon);
  }
  return avatarCache.value.get(id) ?? "";
}

const tierLabel: Record<number, string> = { 1: "便宜", 2: "中档", 3: "贵档" };
const tierClass: Record<number, string> = { 1: "tier-1", 2: "tier-2", 3: "tier-3" };
const tierLeftColor: Record<number, string> = { 1: "#5fd39a", 2: "#d4b06a", 3: "#c79cff" };
const tierGlow: Record<number, string> = { 1: "rgba(95,211,154,.2)", 2: "rgba(212,176,106,.2)", 3: "rgba(199,156,255,.2)" };

const emit = defineEmits<{
  (e: "select", id: string): void;
  (e: "mode-change", mode: string): void;
}>();
</script>

<template>
  <div class="expert-team">
    <!-- 模式选择 — 分段控件（WorkBuddy 风） -->
    <div class="mode-bar">
      <button
        v-for="m in modes"
        :key="m.id"
        class="mode-btn"
        :class="{ active: currentMode === m.id }"
        :title="m.desc"
        @click="selectMode(m.id)"
      >
        <span class="mode-ic" v-html="m.icon" />
        <span class="mode-lb">{{ m.label }}</span>
        <span v-if="currentMode === m.id" class="mode-underline" />
      </button>
    </div>

    <!-- ══ 单Agent ══ -->
    <div v-if="currentMode === 'single-agent'" class="mode-hint">
      <div class="hint-icon" v-html="IconAgent" />
      <p>直接对话，无专家加成，最便宜模式</p>
    </div>

    <!-- ══ 单专家：WorkBuddy 卡片市场 ══ -->
    <div v-else-if="currentMode === 'single-expert'" class="gallery">
      <!-- 分组筛选 -->
      <div class="group-bar">
        <button class="gb-btn" :class="{ on: !selectedGroup }" @click="selectGroup(null)">
          全部 <span class="gb-c">{{ allExperts.length }}</span>
        </button>
        <button
          v-for="g in groups"
          :key="g.id"
          class="gb-btn"
          :class="{ on: selectedGroup === g.id }"
          @click="selectGroup(g.id)"
        >
          <span class="gb-icon">{{ g.icon }}</span>{{ g.name }} <span class="gb-c">{{ g.count }}</span>
        </button>
      </div>

      <!-- 专家卡片网格 -->
      <div class="exp-grid">
        <button
          v-for="exp in filteredExperts"
          :key="exp.id"
          class="exp-card"
          :style="{
            '--tier-left': tierLeftColor[exp.costTier],
            '--tier-glow': tierGlow[exp.costTier],
          }"
          :title="exp.description"
          @click="selectExpert(exp)"
        >
          <div class="exp-avatar">
            <img
              v-if="avatarUrl(exp.id, exp.icon)"
              :src="avatarUrl(exp.id, exp.icon)"
              :alt="exp.name"
              class="exp-img"
            />
            <div v-else class="exp-img-fallback">{{ exp.icon }}</div>
          </div>
          <div class="exp-info">
            <div class="exp-name-row">
              <span class="exp-name">{{ exp.name }}</span>
              <span class="exp-tier" :class="tierClass[exp.costTier]">{{ tierLabel[exp.costTier] }}</span>
            </div>
            <div class="exp-role">{{ exp.role }}</div>
            <div class="exp-tags">
              <span class="exp-comp">{{ exp.complements }}</span>
            </div>
          </div>
        </button>
      </div>
    </div>

    <!-- ══ 专家团：WorkBuddy 团队选卡 ══ -->
    <div v-else-if="currentMode === 'expert-team'" class="team-select">
      <p class="team-hint">选择一支专家团入驻项目，战略师将按任务按需召集</p>
      <div class="team-cards">
        <button
          v-for="team in teams"
          :key="team.id"
          class="team-card"
          @click="selectTeam(team.id)"
        >
          <div class="team-card-icon">{{ team.icon }}</div>
          <div class="team-card-body">
            <div class="team-card-name">{{ team.name }}</div>
            <div class="team-card-desc">{{ team.desc }}</div>
            <div class="team-card-tags">
              <span v-for="tag in team.tags" :key="tag" class="team-tag">{{ tag }}</span>
            </div>
          </div>
          <div class="team-card-arrow">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
          </div>
        </button>
      </div>
    </div>

    <!-- ══ 智能匹配：自然语路由 ══ -->
    <div v-else-if="currentMode === 'auto-match'" class="auto-match">
      <div class="am-input-row">
        <input
          v-model="searchQuery"
          class="am-input"
          placeholder="描述你的需求…如：帮我做一个带支付的 SaaS 落地页"
          @keyup.enter="runAutoMatch(searchQuery)"
        />
        <button class="am-btn" :disabled="loading" @click="runAutoMatch(searchQuery)">
          <span v-html="IconSpark" />
          {{ loading ? "匹配中…" : "智能匹配" }}
        </button>
      </div>

      <!-- 匹配结果 -->
      <div v-if="autoMatchResults.length" class="am-results">
        <div
          v-for="(m, i) in autoMatchResults"
          :key="m.expert.id"
          class="am-result"
          :class="{ primary: i === 0, 'result-enter': true }"
          :style="{
            '--tier-left': tierLeftColor[m.expert.costTier],
            '--tier-glow': tierGlow[m.expert.costTier],
            '--enter-delay': `${i * 60}ms`,
          }"
        >
          <div class="am-rank">{{ i + 1 }}</div>

          <div class="am-avatar-wrap">
            <img
              v-if="avatarUrl(m.expert.id, m.expert.icon)"
              :src="avatarUrl(m.expert.id, m.expert.icon)"
              class="am-avatar"
              :alt="m.expert.name"
            />
            <div v-else class="am-avatar-fallback">{{ m.expert.icon }}</div>
          </div>

          <div class="am-info">
            <div class="am-name-row">
              <span class="am-name">{{ m.expert.name }}</span>
              <span class="am-tier" :class="tierClass[m.expert.costTier]">{{ tierLabel[m.expert.costTier] }}</span>
            </div>
            <div class="am-score">相似度 {{ (m.similarity * 100).toFixed(0) }}%</div>
            <!-- 为什么选你 -->
            <div v-if="m.hitSignals.length || m.complements" class="am-reason">
              <div v-if="m.hitSignals.length" class="am-hits">
                <span class="am-hits-label">命中</span>
                <span v-for="h in m.hitSignals" :key="h" class="am-hit">{{ h }}</span>
              </div>
              <div v-if="m.complements" class="am-comp-row">
                <span class="am-comp-label">补维</span>
                <span class="am-comp-value">{{ m.complements }}</span>
              </div>
            </div>
          </div>

          <button class="am-use" @click="selectExpert(m.expert)">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
            选用
          </button>
        </div>
      </div>
      <div v-else-if="!loading" class="am-empty">
        <div class="am-empty-icon" v-html="IconSpark" />
        <p>描述需求，智能匹配最合适的专家</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.expert-team {
  padding: 14px;
  background: var(--panel, rgba(24,24,34,.66));
  border-radius: 14px;
  border: 1px solid var(--line, rgba(255,255,255,.08));
}

/* ── 分段控件模式栏 ── */
.mode-bar {
  display: flex;
  gap: 3px;
  margin-bottom: 14px;
  background: rgba(255,255,255,.03);
  border-radius: 12px;
  padding: 3px;
  border: 1px solid rgba(255,255,255,.06);
}

.mode-btn {
  flex: 1;
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  padding: 8px 4px 6px;
  border-radius: 9px;
  border: none;
  background: transparent;
  color: var(--dim, #a7a6b4);
  cursor: pointer;
  transition: all .18s;
  font-size: 12px;
}
.mode-btn:hover { color: var(--ink, #e9e8ee); background: rgba(255,255,255,.04); }
.mode-btn.active {
  background: rgba(255,255,255,.09);
  color: var(--ink, #e9e8ee);
}

.mode-underline {
  position: absolute;
  bottom: 4px;
  left: 50%;
  transform: translateX(-50%);
  width: 18px;
  height: 2px;
  border-radius: 2px;
  background: linear-gradient(90deg, #d4b06a, #b07bff);
  box-shadow: 0 0 6px rgba(212,176,106,.4);
}

.mode-ic {
  display: flex;
  align-items: center;
  justify-content: center;
  color: inherit;
}
.mode-lb { font-size: 11px; font-weight: 600; letter-spacing: .3px; }

/* ── 通用 ── */
.mode-hint {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 24px 16px;
  text-align: center;
}
.hint-icon {
  color: var(--faint);
  display: flex;
  align-items: center;
  justify-content: center;
}
.mode-hint p { color: var(--dim, #a7a6b4); font-size: 13px; margin: 0; }

/* ── 分组筛选 ── */
.group-bar {
  display: flex;
  gap: 5px;
  flex-wrap: wrap;
  margin-bottom: 10px;
}
.gb-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border-radius: 20px;
  border: 1px solid var(--line);
  background: transparent;
  color: var(--dim);
  font-size: 12px;
  cursor: pointer;
  transition: .14s;
}
.gb-btn:hover { color: var(--ink); border-color: rgba(212,176,106,.3); }
.gb-btn.on {
  background: linear-gradient(135deg, rgba(212,176,106,.15), rgba(176,123,255,.1));
  border-color: rgba(212,176,106,.45);
  color: var(--ink);
}
.gb-icon { font-size: 11px; }
.gb-c { opacity: .55; font-size: 11px; }

/* ── 专家卡片网格 ── */
.exp-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 7px;
  max-height: 340px;
  overflow-y: auto;
  padding-right: 2px;
}
.exp-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 10px 10px 12px;
  border-radius: 10px;
  border: 1px solid var(--line);
  border-left: 3px solid var(--tier-left, var(--line));
  background: rgba(255,255,255,.025);
  cursor: pointer;
  text-align: left;
  transition: all .16s;
}
.exp-card:hover {
  border-color: rgba(212,176,106,.35);
  border-left-color: var(--tier-left);
  background: rgba(255,255,255,.05);
  transform: translateY(-1px);
  box-shadow: 0 3px 14px rgba(0,0,0,.2), 0 0 0 1px rgba(212,176,106,.1);
}

.exp-avatar { flex: 0 0 44px; display: flex; align-items: center; justify-content: center; }
.exp-img { width: 44px; height: 44px; border-radius: 50%; object-fit: cover; }
.exp-img-fallback {
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--tier-glow, rgba(212,176,106,.2)), rgba(255,255,255,.06));
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
}

.exp-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
.exp-name-row { display: flex; align-items: center; gap: 5px; }
.exp-name { font-size: 13px; font-weight: 600; color: var(--ink); flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.exp-tier {
  font-size: 10px;
  padding: 1px 5px;
  border-radius: 4px;
  border: 1px solid;
  white-space: nowrap;
  flex-shrink: 0;
}
.tier-1 { color: #5fd39a; border-color: rgba(95,211,154,.4); background: rgba(95,211,154,.08); }
.tier-2 { color: #e6c984; border-color: rgba(230,201,132,.4); background: rgba(230,201,132,.08); }
.tier-3 { color: #c79cff; border-color: rgba(199,156,255,.4); background: rgba(199,156,255,.08); }
.exp-role { font-size: 11px; color: var(--dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.exp-tags { display: flex; gap: 4px; margin-top: 2px; }
.exp-comp { font-size: 10px; color: var(--faint); }

/* ── 团队选卡 ── */
.team-hint {
  font-size: 12px;
  color: var(--dim);
  margin: 0 0 12px;
  text-align: center;
}
.team-cards {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.team-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border-radius: 12px;
  border: 1px solid var(--line);
  background: rgba(255,255,255,.025);
  cursor: pointer;
  text-align: left;
  transition: all .16s;
}
.team-card:hover {
  border-color: rgba(212,176,106,.4);
  background: rgba(212,176,106,.06);
  transform: translateY(-1px);
  box-shadow: 0 3px 14px rgba(0,0,0,.2);
}
.team-card-icon { font-size: 28px; flex: 0 0 44px; text-align: center; }
.team-card-body { flex: 1; display: flex; flex-direction: column; gap: 2px; }
.team-card-name { font-size: 14px; font-weight: 700; color: var(--ink); }
.team-card-desc { font-size: 12px; color: var(--dim); }
.team-card-tags { display: flex; gap: 4px; margin-top: 4px; }
.team-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  background: rgba(176,123,255,.12);
  border: 1px solid rgba(176,123,255,.3);
  color: #b07bff;
}
.team-card-arrow { color: var(--faint); display: flex; align-items: center; transition: .14s; }
.team-card:hover .team-card-arrow { color: var(--gold, #d4b06a); transform: translateX(2px); }

/* ── 智能匹配 ── */
.am-input-row { display: flex; gap: 8px; margin-bottom: 12px; }
.am-input {
  flex: 1;
  padding: 9px 12px;
  border-radius: 10px;
  border: 1px solid var(--line);
  background: rgba(255,255,255,.05);
  color: var(--ink);
  font-size: 13px;
  outline: none;
  transition: border-color .16s;
}
.am-input:focus { border-color: rgba(212,176,106,.4); }
.am-input::placeholder { color: var(--faint); }
.am-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 9px 14px;
  border-radius: 10px;
  background: linear-gradient(135deg, #d4b06a, #b07bff);
  border: none;
  color: #15131a;
  font-weight: 700;
  font-size: 12px;
  cursor: pointer;
  white-space: nowrap;
  transition: opacity .14s, transform .14s;
}
.am-btn:hover { transform: translateY(-1px); box-shadow: 0 3px 10px rgba(176,123,255,.3); }
.am-btn:disabled { opacity: .5; cursor: not-allowed; transform: none; }

/* 结果入场动画 */
.result-enter {
  animation: slideInUp 260ms ease-out both;
  animation-delay: var(--enter-delay, 0ms);
}
@keyframes slideInUp {
  from { opacity: 0; transform: translateY(8px); }
  to   { opacity: 1; transform: translateY(0); }
}

.am-results { display: flex; flex-direction: column; gap: 7px; }
.am-result {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 10px 10px 12px;
  border-radius: 10px;
  border: 1px solid var(--line);
  border-left: 3px solid var(--tier-left, var(--line));
  background: rgba(255,255,255,.025);
  transition: all .14s;
}
.am-result.primary {
  border-color: rgba(212,176,106,.5);
  border-left-color: #d4b06a;
  background: rgba(212,176,106,.07);
  box-shadow: 0 0 0 1px rgba(212,176,106,.12), 0 2px 12px rgba(0,0,0,.18);
}

.am-rank { font-size: 18px; font-weight: 800; color: var(--faint); width: 18px; text-align: center; flex: 0 0 18px; }
.am-result.primary .am-rank { color: #d4b06a; }

.am-avatar-wrap { flex: 0 0 42px; display: flex; align-items: center; justify-content: center; }
.am-avatar { width: 42px; height: 42px; border-radius: 50%; object-fit: cover; }
.am-avatar-fallback {
  width: 42px;
  height: 42px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--tier-glow, rgba(212,176,106,.2)), rgba(255,255,255,.06));
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
}

.am-info { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.am-name-row { display: flex; align-items: center; gap: 5px; }
.am-name { font-size: 13px; font-weight: 600; color: var(--ink); flex: 1; }
.am-tier {
  font-size: 10px;
  padding: 1px 5px;
  border-radius: 4px;
  border: 1px solid;
  flex-shrink: 0;
}
.am-score { font-size: 11px; color: var(--gold, #d4b06a); }

/* 为什么选你 */
.am-reason { display: flex; flex-direction: column; gap: 2px; margin-top: 3px; }
.am-hits { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
.am-hits-label { font-size: 10px; color: var(--faint); margin-right: 2px; }
.am-hit {
  font-size: 10px;
  background: rgba(110,168,255,.12);
  border: 1px solid rgba(110,168,255,.28);
  color: #6ea8ff;
  padding: 1px 5px;
  border-radius: 4px;
}
.am-comp-row { display: flex; align-items: center; gap: 4px; }
.am-comp-label { font-size: 10px; color: var(--faint); }
.am-comp-value { font-size: 11px; color: var(--dim); }

.am-use {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 12px;
  border-radius: 8px;
  border: 1px solid rgba(212,176,106,.4);
  background: transparent;
  color: var(--gold, #d4b06a);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all .14s;
  flex-shrink: 0;
}
.am-use:hover { background: rgba(212,176,106,.15); transform: translateY(-1px); }

.am-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 32px 16px;
  text-align: center;
}
.am-empty-icon {
  color: var(--faint);
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: .5;
}
.am-empty p { color: var(--faint); font-size: 13px; margin: 0; }
</style>
