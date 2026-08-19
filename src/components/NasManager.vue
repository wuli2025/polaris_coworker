<script setup lang="ts">
/**
 * 盘管理 —— NAS 网络盘(SMB)的记忆与一键映射。
 *
 * 统一展示两类盘：本机 SMB 网络盘，以及由「互联」通过 iroh 接入的设备盘。
 * SMB 仍走 nas.* 记忆/映射；互联盘只复用 remoteSources + fsmount 状态并跳文件中心浏览，
 * 两套传输契约不混用。SMB 凭据仅存本机(~/Polaris/data/nas.json)，不上传。
 */
import { ref, reactive, computed, onMounted } from "vue";
import {
  X,
  Server,
  HardDrive,
  Plus,
  Trash2,
  Link2,
  Unlink,
  Pencil,
  RefreshCw,
  Check,
  Info,
  Network,
  FolderOpen,
} from "@lucide/vue";
import OrbitSpinner from "./icons/OrbitSpinner.vue";
import {
  invoke,
  nas,
  fsmount,
  type NasView,
  type NasRecord,
  type MountStatus,
} from "../tauri";
import { loadRemoteSources, type RemoteSource } from "../features/interconnect/remoteSources";

interface MeshPeer {
  nodeId?: string;
  name?: string;
  port?: number;
  token?: string;
  connected?: boolean;
  error?: string;
}
interface MeshStatus {
  peers?: MeshPeer[];
}
type ManagedRemote = RemoteSource & {
  mesh: boolean;
  connected: boolean;
  error: string;
};

const emit = defineEmits<{
  (e: "close"): void;
  (e: "browse-remote", source: RemoteSource): void;
  (e: "open-interconnect"): void;
}>();

const list = ref<NasView[]>([]);
const remoteSources = ref<ManagedRemote[]>([]);
const mounts = ref<MountStatus[]>([]);
const loading = ref(false);
const busyId = ref<string | null>(null);
const toast = reactive({ show: false, text: "", err: false });

// 编辑表单(新增 / 改一条)。drive 单字母,空 = 自动挑盘符。
const blank = (): Partial<NasRecord> & { keepPassword?: boolean } => ({
  id: "",
  label: "",
  host: "",
  share: "",
  username: "",
  password: "",
  drive: "",
  persistent: true,
  keepPassword: false,
});
const form = reactive(blank());
const editing = ref(false);
const editHasPassword = ref(false);

const saved = computed(() => list.value.filter((n) => !n.discovered));
const discovered = computed(() => list.value.filter((n) => n.discovered));
const mountBySource = computed(() =>
  Object.fromEntries(mounts.value.map((m) => [m.sourceId, m]))
);

/**
 * 互联有两条现役入口：老的手工 NodeId 源存在 localStorage；同账号设备和新设备码则由
 * mesh_status 返回，后端已经直接起隧道并挂盘。这里按 NodeId 合流，优先使用 mesh 的实时
 * 端口/token/sourceId，避免“盘已挂上，盘管理却说没有”的分裂状态。
 */
function mergeRemoteSources(peers: MeshPeer[]): ManagedRemote[] {
  const byPeer = new Map<string, ManagedRemote>();
  for (const source of loadRemoteSources()) {
    const key = source.nodeId ? `node:${source.nodeId}` : `id:${source.id}`;
    byPeer.set(key, {
      ...source,
      mesh: false,
      connected: false,
      error: "",
    });
  }
  for (const peer of peers) {
    const nodeId = peer.nodeId?.trim() || "";
    if (!nodeId) continue;
    const key = `node:${nodeId}`;
    const existing = byPeer.get(key);
    const live = !!peer.token && Number(peer.port) > 0;
    if (existing && !live) {
      byPeer.set(key, {
        ...existing,
        mesh: true,
        connected: !!peer.connected,
        error: peer.error || "",
      });
      continue;
    }
    byPeer.set(key, {
      id: `mesh-${nodeId.slice(0, 16)}`,
      name: peer.name?.trim() || existing?.name || "互联设备盘",
      nodeId,
      port: Number(peer.port) || 0,
      token: peer.token || "",
      createdAt: existing?.createdAt || 0,
      mesh: true,
      connected: !!peer.connected,
      error: peer.error || "",
    });
  }
  return [...byPeer.values()];
}

function mountOf(id: string): MountStatus | undefined {
  return mountBySource.value[id];
}

function canBrowse(source: ManagedRemote): boolean {
  return !!source.token && source.port > 0;
}

function remoteState(source: ManagedRemote): string {
  const mounted = mountOf(source.id);
  if (mounted?.ok && mounted.drive) {
    return `${mounted.drive} · ${mounted.writable ? "读写" : "只读"}`;
  }
  if (mounted?.error) return `等待重连 · ${mounted.error}`;
  if (source.error) return `等待重连 · ${source.error}`;
  if (source.mesh && !source.connected) return "正在连接";
  return canBrowse(source) ? "可在文件中心浏览" : "等待连接就绪";
}

function browseRemote(source: ManagedRemote) {
  if (!canBrowse(source)) return;
  emit("browse-remote", source);
}

function flash(text: string, err = false) {
  toast.text = text;
  toast.err = err;
  toast.show = true;
  window.setTimeout(() => (toast.show = false), err ? 5200 : 3200);
}

async function refresh() {
  loading.value = true;
  try {
    const [nasResult, mountResult, meshResult] = await Promise.allSettled([
      nas.list(),
      fsmount.status(),
      invoke<MeshStatus>("mesh_status"),
    ]);
    if (nasResult.status === "fulfilled") list.value = nasResult.value;
    else flash(String(nasResult.reason), true);
    // 老版本 / Web 壳可能没有系统挂载或 mesh 命令；另两类盘仍各自照常显示。
    mounts.value = mountResult.status === "fulfilled" ? mountResult.value : [];
    const peers = meshResult.status === "fulfilled" ? meshResult.value.peers ?? [] : [];
    remoteSources.value = mergeRemoteSources(peers);
  } finally {
    loading.value = false;
  }
}

function openNew() {
  Object.assign(form, blank());
  editHasPassword.value = false;
  editing.value = true;
}

function openEdit(n: NasView) {
  Object.assign(form, {
    id: n.id,
    label: n.label,
    host: n.host,
    share: n.share,
    username: n.username,
    password: "",
    drive: n.drive,
    persistent: n.persistent,
    keepPassword: n.hasPassword,
  });
  editHasPassword.value = n.hasPassword;
  editing.value = true;
}

function cancelEdit() {
  editing.value = false;
}

async function submitForm() {
  if (!form.host?.trim()) {
    flash("请填主机地址(IP 或主机名)", true);
    return;
  }
  try {
    const rec: Partial<NasRecord> = {
      id: form.id || "",
      label: form.label || "",
      host: form.host || "",
      share: form.share || "",
      username: form.username || "",
      drive: form.drive || "",
      persistent: !!form.persistent,
    };
    // 留空密码 = 沿用旧密码(后端处理);只有真填了才发。
    if (form.password) rec.password = form.password;
    await nas.save(rec);
    editing.value = false;
    flash("已保存");
    await refresh();
  } catch (e) {
    flash(String(e), true);
  }
}

/** 连接一条:已有完整凭据直接连;发现的/缺密码的先带出表单让用户补。 */
async function connect(n: NasView) {
  // 发现的连接没存密码,但 Windows 可能记得 → 先尝试无密码直连,失败再弹表单补。
  busyId.value = n.id;
  try {
    const msg = await nas.connect({
      id: n.id,
      label: n.label,
      host: n.host,
      share: n.share,
      username: n.username,
      drive: n.drive,
      persistent: n.persistent,
    });
    flash(msg);
    await refresh();
  } catch (e) {
    const msg = String(e);
    // 凭据不足 → 引导补全
    flash(msg + "(如需账号密码,请点「编辑」补全后再连)", true);
    openEdit(n);
  } finally {
    busyId.value = null;
  }
}

async function disconnect(n: NasView) {
  busyId.value = n.id;
  try {
    const msg = await nas.disconnect({ host: n.host, share: n.share });
    flash(msg);
    await refresh();
  } catch (e) {
    flash(String(e), true);
  } finally {
    busyId.value = null;
  }
}

async function forget(n: NasView) {
  busyId.value = n.id;
  try {
    const msg = await nas.forget(n.id);
    flash(msg);
    await refresh();
  } catch (e) {
    flash(String(e), true);
  } finally {
    busyId.value = null;
  }
}

function fmtTime(secs: number | null): string {
  if (!secs) return "";
  try {
    const d = new Date(secs * 1000);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  } catch {
    return "";
  }
}

onMounted(refresh);
</script>

<template>
  <div class="nas-overlay" @click.self="emit('close')">
    <div class="nas-panel glass">
      <header class="nas-head">
        <div class="nas-title">
          <Server :size="18" :stroke-width="1.8" />
          <span>盘管理 · 本机与互联</span>
        </div>
        <div class="nas-head-actions">
          <button class="ghost-btn" :disabled="loading" title="刷新" @click="refresh">
            <OrbitSpinner v-if="loading" :size="15" />
            <RefreshCw v-else :size="15" :stroke-width="1.8" />
          </button>
          <button class="ghost-btn" title="关闭" @click="emit('close')">
            <X :size="17" :stroke-width="2" />
          </button>
        </div>
      </header>

      <div class="nas-sub">
        <Info :size="13" :stroke-width="1.8" />
        <span>这里统一管理本机 SMB 网络盘和「互联」接入的设备盘。连上后可直接浏览，也能被文件中心盘点。</span>
      </div>

      <div class="nas-body">
        <!-- 互联设备盘：数据源与互联页完全相同；这里只提供浏览和去互联管理，
             不拿 iroh 连接去调用 SMB 的 connect/disconnect。 -->
        <section class="nas-sect remote-section">
          <div class="sect-head">
            <div>
              <h4>互联设备盘</h4>
              <span class="sect-note">另一台电脑 / NAS 用同一邮箱接入后会自动出现在这里</span>
            </div>
            <button class="add-btn" @click="emit('open-interconnect')">
              <Network :size="14" :stroke-width="2" /> 去互联添加
            </button>
          </div>
          <div v-if="!remoteSources.length" class="empty-hint remote-empty">
            暂无互联设备盘。到「互联」用同一邮箱登录另一台设备，或输入它的设备码；连接成功后这里会同步出现。
          </div>
          <ul v-else class="nas-list">
            <li v-for="source in remoteSources" :key="source.id" class="nas-card remote-card" :class="{ on: mountOf(source.id)?.ok }">
              <div class="card-ic"><Network :size="20" :stroke-width="1.7" /></div>
              <div class="card-main">
                <div class="card-top">
                  <span class="card-name">{{ source.name || "互联设备盘" }}</span>
                  <span v-if="mountOf(source.id)?.ok" class="badge ok">已连接</span>
                </div>
                <div class="card-unc">互联设备 · {{ source.nodeId.slice(0, 8) }}…{{ source.nodeId.slice(-6) }}</div>
                <div class="card-meta" :class="{ warn: !!mountOf(source.id)?.error }">
                  <span>{{ remoteState(source) }}</span>
                </div>
              </div>
              <div class="card-acts">
                <button
                  class="act primary"
                  :disabled="!canBrowse(source)"
                  :title="canBrowse(source) ? '在文件中心浏览' : '连接就绪后可浏览'"
                  @click="browseRemote(source)"
                >
                  <FolderOpen :size="14" :stroke-width="1.9" /> 浏览
                </button>
                <button class="act" @click="emit('open-interconnect')">
                  <Network :size="14" :stroke-width="1.9" /> 管理
                </button>
              </div>
            </li>
          </ul>
        </section>

        <!-- 编辑/新增本机 SMB 表单 -->
        <section v-if="editing" class="nas-form glass-in">
          <div class="form-grid">
            <label class="fld">
              <span>主机地址 *</span>
              <input v-model.trim="form.host" placeholder="IP 或主机名,如 100.78.103.101 / DiskStation" />
            </label>
            <label class="fld">
              <span>共享名</span>
              <input v-model.trim="form.share" placeholder="NAS 上要挂载的共享文件夹,如 tx" />
            </label>
            <label class="fld">
              <span>显示名</span>
              <input v-model.trim="form.label" placeholder="给它起个名,如 群晖 · 资料" />
            </label>
            <label class="fld">
              <span>账号</span>
              <input v-model.trim="form.username" placeholder="登录 NAS 的用户名(匿名共享可空)" autocomplete="off" />
            </label>
            <label class="fld">
              <span>密码</span>
              <input
                v-model="form.password"
                type="password"
                :placeholder="editHasPassword ? '已保存,留空 = 沿用旧密码' : '登录 NAS 的密码(匿名共享可空)'"
                autocomplete="new-password"
              />
            </label>
            <label class="fld fld-sm">
              <span>盘符</span>
              <input v-model.trim="form.drive" maxlength="1" placeholder="Z" />
            </label>
          </div>
          <label class="chk">
            <input v-model="form.persistent" type="checkbox" />
            <span>重启后保持映射(下次开机自动重连)</span>
          </label>
          <div class="form-actions">
            <button class="btn-ghost" @click="cancelEdit">取消</button>
            <button class="btn-primary" @click="submitForm"><Check :size="15" :stroke-width="2" /> 保存</button>
          </div>
        </section>

        <!-- 已记住的本机 SMB 网络盘 -->
        <section class="nas-sect">
          <div class="sect-head">
            <h4>本机网络盘（SMB）</h4>
            <button v-if="!editing" class="add-btn" @click="openNew"><Plus :size="14" :stroke-width="2" /> 添加网络盘</button>
          </div>
          <div v-if="!saved.length" class="empty-hint">
            还没有记住的 SMB 网络盘。点「添加网络盘」填一次主机 / 共享 / 账号，以后一键连接。
          </div>
          <ul v-else class="nas-list">
            <li v-for="n in saved" :key="n.id" class="nas-card" :class="{ on: n.connected }">
              <div class="card-ic"><HardDrive :size="20" :stroke-width="1.6" /></div>
              <div class="card-main">
                <div class="card-top">
                  <span class="card-name">{{ n.label }}</span>
                  <span v-if="n.connected" class="badge ok">已连接 · {{ n.currentDrive }}:</span>
                </div>
                <div class="card-unc">{{ n.unc }}</div>
                <div class="card-meta">
                  <span>{{ n.status }}</span>
                  <span v-if="n.lastConnected" class="dot">·</span>
                  <span v-if="n.lastConnected">上次 {{ fmtTime(n.lastConnected) }}</span>
                </div>
              </div>
              <div class="card-acts">
                <button
                  v-if="!n.connected"
                  class="act primary"
                  :disabled="busyId === n.id"
                  title="映射成网络盘"
                  @click="connect(n)"
                >
                  <OrbitSpinner v-if="busyId === n.id" :size="14" />
                  <Link2 v-else :size="14" :stroke-width="1.9" /> 连接
                </button>
                <button
                  v-else
                  class="act"
                  :disabled="busyId === n.id"
                  title="取消映射"
                  @click="disconnect(n)"
                >
                  <OrbitSpinner v-if="busyId === n.id" :size="14" />
                  <Unlink v-else :size="14" :stroke-width="1.9" /> 断开
                </button>
                <button class="act icon" title="编辑" @click="openEdit(n)"><Pencil :size="14" :stroke-width="1.8" /></button>
                <button class="act icon danger" title="忘记" :disabled="busyId === n.id" @click="forget(n)"><Trash2 :size="14" :stroke-width="1.8" /></button>
              </div>
            </li>
          </ul>
        </section>

        <!-- 系统里发现的(之前登陆过 / 远程登陆过)-->
        <section v-if="discovered.length" class="nas-sect">
          <div class="sect-head">
            <h4>系统里发现的连接</h4>
            <span class="sect-note">你之前登陆过、Windows 还记着的网络盘 · 连一下即记住</span>
          </div>
          <ul class="nas-list">
            <li v-for="n in discovered" :key="n.id" class="nas-card discovered" :class="{ on: n.connected }">
              <div class="card-ic"><Server :size="19" :stroke-width="1.6" /></div>
              <div class="card-main">
                <div class="card-top">
                  <span class="card-name">{{ n.label }}</span>
                  <span v-if="n.connected" class="badge ok">已连接 · {{ n.currentDrive }}:</span>
                </div>
                <div class="card-unc">{{ n.unc }}</div>
                <div class="card-meta"><span>{{ n.status }}</span></div>
              </div>
              <div class="card-acts">
                <button
                  v-if="!n.connected"
                  class="act primary"
                  :disabled="busyId === n.id"
                  @click="connect(n)"
                >
                  <OrbitSpinner v-if="busyId === n.id" :size="14" />
                  <Link2 v-else :size="14" :stroke-width="1.9" /> 连接
                </button>
                <button v-else class="act" :disabled="busyId === n.id" @click="disconnect(n)">
                  <OrbitSpinner v-if="busyId === n.id" :size="14" />
                  <Unlink v-else :size="14" :stroke-width="1.9" /> 断开
                </button>
                <button class="act icon" title="完善并记住" @click="openEdit(n)"><Pencil :size="14" :stroke-width="1.8" /></button>
              </div>
            </li>
          </ul>
        </section>
      </div>

      <transition name="toast">
        <div v-if="toast.show" class="nas-toast" :class="{ err: toast.err }">{{ toast.text }}</div>
      </transition>
    </div>
  </div>
</template>

<style scoped>
.nas-overlay {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--overlay);
  backdrop-filter: blur(6px);
  padding: 24px;
}
.nas-panel {
  width: min(860px, 96vw);
  max-height: 88vh;
  display: flex;
  flex-direction: column;
  color: var(--text);
  color-scheme: light;
  border-radius: 20px;
  border: 1px solid var(--border);
  background: color-mix(in srgb, var(--panel) 96%, transparent);
  backdrop-filter: blur(26px) saturate(1.3);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
  position: relative;
}
:global(html[data-theme="dark"]) .nas-panel,
:global(html[data-theme="aurora-dark"]) .nas-panel {
  color-scheme: dark;
}
.nas-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 18px 12px;
}
.nas-title {
  display: flex;
  align-items: center;
  gap: 9px;
  font-size: 15.5px;
  font-weight: 650;
  letter-spacing: 0.2px;
}
.nas-head-actions {
  display: flex;
  gap: 6px;
}
.ghost-btn {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  border-radius: 10px;
  border: 1px solid var(--border-soft);
  background: var(--bg-soft);
  color: var(--text-2);
  cursor: pointer;
  transition: background 0.16s, border-color 0.16s;
}
.ghost-btn:hover {
  background: var(--selection-bg-hover);
  color: var(--text);
}
.nas-sub {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  margin: 0 18px 8px;
  padding: 9px 11px;
  font-size: 12.3px;
  line-height: 1.55;
  color: var(--text-2);
  background: var(--primary-soft);
  border: 1px solid color-mix(in srgb, var(--primary) 20%, transparent);
  border-radius: 11px;
}
.nas-sub svg {
  flex: 0 0 auto;
  margin-top: 2px;
  opacity: 0.85;
}
.nas-body {
  padding: 6px 18px 20px;
  overflow-y: auto;
}

/* 表单 */
.nas-form {
  margin: 6px 0 16px;
  padding: 14px;
  border-radius: 14px;
  border: 1px solid var(--border);
  background: var(--bg-soft);
}
.glass-in {
  animation: glassIn 0.18s ease;
}
@keyframes glassIn {
  from { opacity: 0; transform: translateY(-4px); }
  to { opacity: 1; transform: none; }
}
.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 11px 13px;
}
.fld {
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: 12px;
}
.fld.fld-sm {
  grid-column: auto;
  max-width: 120px;
}
.fld > span {
  color: var(--text-2);
  font-weight: 600;
}
.fld input {
  height: 36px;
  padding: 0 11px;
  border-radius: 9px;
  border: 1px solid var(--border);
  background: var(--panel);
  color: var(--text);
  font-size: 13px;
  outline: none;
  transition: border-color 0.16s, box-shadow 0.16s;
}
.fld input::placeholder {
  color: var(--dim);
  opacity: 1;
}
.fld input:focus {
  border-color: var(--primary);
  box-shadow: 0 0 0 3px var(--primary-soft);
}
.chk {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
  font-size: 12.5px;
  color: var(--text-2);
  cursor: pointer;
}
.chk input {
  width: 15px;
  height: 15px;
  accent-color: #6aa0ff;
}
.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: 9px;
  margin-top: 14px;
}
.btn-ghost,
.btn-primary {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 34px;
  padding: 0 16px;
  border-radius: 10px;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  border: 1px solid transparent;
  transition: filter 0.16s, background 0.16s;
}
.btn-ghost {
  background: var(--panel);
  border-color: var(--border);
  color: var(--text-2);
}
.btn-ghost:hover {
  background: var(--selection-bg-hover);
  color: var(--text);
}
.btn-primary {
  background: linear-gradient(135deg, #5b8cff, #6f6aff);
  color: #fff;
}
.btn-primary:hover {
  filter: brightness(1.08);
}

/* 区块 */
.nas-sect {
  margin-top: 10px;
}
.sect-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  margin: 14px 2px 9px;
}
.sect-head h4 {
  margin: 0;
  font-size: 12.5px;
  font-weight: 650;
  letter-spacing: 0.3px;
  color: var(--text-2);
}
.sect-note {
  font-size: 11.3px;
  color: var(--muted);
}
.add-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 28px;
  padding: 0 11px;
  border-radius: 9px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  color: var(--primary);
  background: var(--primary-soft);
  border: 1px solid color-mix(in srgb, var(--primary) 25%, transparent);
  transition: background 0.16s;
}
.add-btn:hover {
  background: color-mix(in srgb, var(--primary) 18%, var(--panel));
}
.empty-hint {
  padding: 18px 14px;
  font-size: 12.6px;
  line-height: 1.6;
  text-align: center;
  color: var(--muted);
  border: 1px dashed var(--border);
  border-radius: 12px;
}

.remote-section {
  margin: 6px 0 16px;
  padding: 0 12px 12px;
  border: 1px solid color-mix(in srgb, var(--primary) 22%, var(--border));
  border-radius: 14px;
  background: color-mix(in srgb, var(--primary-soft) 54%, var(--panel));
}
.remote-section .sect-head > div {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.remote-empty {
  background: var(--panel);
}
.remote-card {
  background: color-mix(in srgb, var(--panel) 92%, var(--primary-soft));
}

/* 卡片 */
.nas-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 9px;
}
.nas-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 13px;
  border-radius: 13px;
  border: 1px solid var(--border-soft);
  background: var(--panel);
  transition: border-color 0.16s, background 0.16s, transform 0.16s;
}
.nas-card:hover {
  background: var(--panel-hover);
}
.nas-card.on {
  border-color: color-mix(in srgb, var(--ok) 42%, var(--border));
  background: var(--ok-soft);
}
.nas-card.discovered {
  border-style: dashed;
}
.card-ic {
  display: grid;
  place-items: center;
  width: 42px;
  height: 42px;
  flex: 0 0 auto;
  border-radius: 11px;
  color: var(--primary);
  background: var(--primary-soft);
}
.nas-card.on .card-ic {
  color: var(--ok);
  background: var(--ok-soft);
}
.card-main {
  flex: 1 1 auto;
  min-width: 0;
}
.card-top {
  display: flex;
  align-items: center;
  gap: 8px;
}
.card-name {
  font-size: 13.7px;
  font-weight: 620;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.badge {
  flex: 0 0 auto;
  font-size: 10.8px;
  font-weight: 650;
  padding: 1.5px 7px;
  border-radius: 999px;
}
.badge.ok {
  color: var(--ok);
  background: var(--ok-soft);
}
.card-unc {
  margin-top: 2px;
  font-size: 11.8px;
  font-family: ui-monospace, "SF Mono", Menlo, monospace;
  color: var(--muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.card-meta {
  margin-top: 3px;
  display: flex;
  gap: 6px;
  font-size: 11.3px;
  color: var(--muted);
}
.card-meta.warn {
  color: var(--vermilion);
}
.card-meta .dot {
  opacity: 0.5;
}
.card-acts {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 0 0 auto;
}
.act {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 30px;
  padding: 0 11px;
  border-radius: 9px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  color: var(--text-2);
  background: var(--bg-soft);
  border: 1px solid var(--border);
  transition: background 0.16s, filter 0.16s;
}
.act:hover {
  color: var(--text);
  background: var(--selection-bg-hover);
}
.act:disabled {
  opacity: 0.55;
  cursor: default;
}
.act.primary {
  color: #fff;
  background: linear-gradient(135deg, #4f86ff, #6f6aff);
  border-color: transparent;
}
.act.primary:hover {
  filter: brightness(1.1);
}
.act.icon {
  padding: 0 9px;
}
.act.icon.danger:hover {
  color: #ff8a8a;
  background: rgba(255, 90, 90, 0.14);
}

/* toast */
.nas-toast {
  position: absolute;
  left: 50%;
  bottom: 16px;
  transform: translateX(-50%);
  max-width: 88%;
  padding: 9px 15px;
  border-radius: 11px;
  font-size: 12.6px;
  line-height: 1.5;
  text-align: center;
  color: #eaf0ff;
  background: rgba(40, 90, 70, 0.92);
  border: 1px solid rgba(120, 230, 170, 0.4);
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.35);
}
.nas-toast.err {
  background: rgba(110, 40, 50, 0.94);
  border-color: rgba(255, 130, 130, 0.42);
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.2s, transform 0.2s;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(8px);
}

@media (max-width: 680px) {
  .nas-overlay { align-items: flex-end; padding: 8px; }
  .nas-panel { width: 100%; max-height: 94vh; border-radius: 17px 17px 10px 10px; }
  .form-grid { grid-template-columns: 1fr; }
  .fld.fld-sm { max-width: none; }
  .sect-head { align-items: flex-start; }
  .nas-card { align-items: flex-start; flex-wrap: wrap; }
  .card-main { min-width: calc(100% - 56px); }
  .card-acts { width: 100%; justify-content: flex-end; }
}

.spin {
  animation: spin 0.9s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
