/**
 * 电脑上的项目(手机侧)—— 「跟 AI 说一句就进项目」的状态层。
 *
 * 手机是瘦客户端:项目、项目绑定的文件夹、以及在文件夹里干活的 claude 全在主机(电脑/
 * 云服务器)上。手机这边只需要记住「现在在哪个项目里」,并让**新对话建在该项目下** ——
 * 主机侧 chat 管线会把该项目的 work_dir 当作 claude 的 cwd(等同电脑上 `cd <repo> &&
 * claude`),于是手机上说的每句话都落在这个项目的代码/文件里。
 *
 * 为什么必须显式建会话:手机原先用本地生成的 `m-<时间戳>` 作 conversationId,主机收到
 * 不认识的 id 会自动把它挂到「第一个未归档项目」下(conv::ensure_writable_or_create)——
 * 那是随机的,用户想开的项目根本轮不上。选了项目后改走 conv_create_conversation,
 * 拿主机发的真 id,cwd 才准。
 *
 * 本文件**不 import chat.ts**(chat.ts 反过来要读这里的 activeProject),避免循环依赖;
 * 「切项目 + 开新会话」的动作在 chat.ts 的 switchProject 里。
 */
import { ref } from "vue";
import { invoke } from "./net";
import { activeHostId } from "./hosts";

export interface WorkProject {
  id: string;
  name: string;
  /** 绑定的电脑目录(绝对路径)。空 = 没绑,对话回落主机默认目录。 */
  work_dir?: string | null;
  created_at?: number;
  archived?: boolean;
  persona_id?: string | null;
  kb_scope?: string | null;
}

/** 当前主机上的项目清单(conv_list_projects 已过滤归档)。 */
export const projects = ref<WorkProject[]>([]);
export const loading = ref(false);
/** 当前进入的项目(null = 没选,行为同旧版:主机自己挑默认项目)。 */
export const activeProject = ref<WorkProject | null>(null);

// 选中的项目按主机分开记(每台主机的项目是各自的,切回来还在原处)。
const pinKey = (hostId: string) => `polaris.m.project.${hostId}`;

function loadPinned(hostId: string): WorkProject | null {
  if (!hostId) return null;
  try {
    const raw = localStorage.getItem(pinKey(hostId));
    const v = raw ? (JSON.parse(raw) as WorkProject) : null;
    return v && typeof v.id === "string" ? v : null;
  } catch {
    return null;
  }
}
function savePinned(hostId: string, p: WorkProject | null): void {
  if (!hostId) return;
  if (p) localStorage.setItem(pinKey(hostId), JSON.stringify(p));
  else localStorage.removeItem(pinKey(hostId));
}

activeProject.value = loadPinned(activeHostId.value);

/** 只记住选择(不动会话)。切项目请用 chat.ts 的 switchProject。 */
export function setActiveProject(p: WorkProject | null): void {
  activeProject.value = p;
  savePinned(activeHostId.value, p);
}

// 拉取代际:切主机/重复点击时让在途的旧结果作废,不串台(与 chat.refreshRemoteConvs 同法)。
let fetchGen = 0;

/** 拉主机的项目清单;顺带把选中项目刷成最新(名字/工作目录可能在电脑上改过)。 */
export async function loadProjects(): Promise<WorkProject[]> {
  loading.value = true;
  const myGen = ++fetchGen;
  const forHost = activeHostId.value;
  try {
    const list = (await invoke<WorkProject[]>("conv_list_projects")) ?? [];
    if (myGen !== fetchGen || forHost !== activeHostId.value) return projects.value;
    projects.value = list;
    const cur = activeProject.value;
    if (cur) {
      const fresh = list.find((p) => p.id === cur.id);
      // 项目在电脑上被归档/删了 → 松开选中,回落主机默认行为(别把会话建到不存在的项目上)。
      if (fresh) setActiveProject(fresh);
      else setActiveProject(null);
    }
    return list;
  } finally {
    if (myGen === fetchGen) loading.value = false;
  }
}

/** 切主机后复位(由 chat.reloadForHost 调用:那时 activeHostId 已指向新主机)。 */
export function resetForHost(): void {
  fetchGen++; // 旧主机在途的拉取作废
  projects.value = [];
  loading.value = false;
  activeProject.value = loadPinned(activeHostId.value);
}

/** 给项目绑定/解绑电脑上的文件夹(主机侧校验目录真实存在)。 */
export async function setWorkDir(projectId: string, workDir: string | null): Promise<void> {
  await invoke("conv_set_project_work_dir", {
    projectId,
    workDir: workDir && workDir.trim() ? workDir.trim() : null,
  });
  const patch = (p: WorkProject) =>
    p.id === projectId ? { ...p, work_dir: workDir?.trim() || null } : p;
  projects.value = projects.value.map(patch);
  if (activeProject.value?.id === projectId) {
    setActiveProject(patch(activeProject.value));
  }
}

/** 在主机上新建项目(可同时绑一个文件夹)。 */
export async function createProject(
  name: string,
  workDir?: string | null
): Promise<WorkProject> {
  const p = await invoke<WorkProject>("conv_create_project", { name });
  projects.value = [...projects.value, p];
  if (workDir && workDir.trim()) {
    await setWorkDir(p.id, workDir);
    return { ...p, work_dir: workDir.trim() };
  }
  return p;
}

/** 工作目录的末段目录名(列表里显示用)。 */
export function dirName(p?: string | null): string {
  if (!p) return "";
  const parts = p.replace(/[\\/]+$/, "").split(/[\\/]+/);
  return parts[parts.length - 1] ?? "";
}

export { parseProjectIntent, matchProject, type ProjectIntent } from "./projectIntent";
