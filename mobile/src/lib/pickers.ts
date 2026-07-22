/**
 * 输入区选择器数据 —— 豆包式输入条的「模型 / 技能」两张列表。
 *
 * 都是只读列表,从主机拉取:
 *  - provider_list:桌面主机走脱敏版(apihub dispatch_desktop,只有 id/name/hasKey 等,
 *    绝无密钥);NAS server 版是全量同名命令,这里只读用得到的字段。
 *  - list_skills:技能市场元数据,只展示已安装(installed)的。
 *
 * 主机老版本没这两个命令 → state 记 unsupported,输入条隐藏对应按钮,聊天不受影响。
 *
 * 选中态(chosen*)是「我的输入习惯」:跨会话沿用、新对话不清,只在切主机时复位 ——
 * 与豆包一致:选好的模型/技能跟人走,不跟单条对话走。
 */
import { ref } from "vue";
import { invoke } from "./net";

export interface ProviderLite {
  id: string;
  name: string;
  category?: string;
  hasKey?: boolean;
  color?: string;
}
export interface SkillLite {
  id: string;
  name: string;
  description: string;
  installed?: boolean;
  category?: string;
}

export type PickState = "idle" | "loading" | "ready" | "unsupported";

export const providers = ref<ProviderLite[]>([]);
/** 主机全局当前供应商 id(Auto 档实际会用的那家)。 */
export const hostCurrentProvider = ref("");
export const providerState = ref<PickState>("idle");

export const skills = ref<SkillLite[]>([]);
export const skillState = ref<PickState>("idle");

// ── 选中态 ──
/** "auto" = 跟随主机全局;具体 id = 本机会话钉这家。 */
export const chosenProviderId = ref("auto");
export const chosenSkillIds = ref<string[]>([]);
/** 豆包「深度思考」同款开关:开 = work 模式(全工具+全约定),关 = fast。 */
export const deepWork = ref(false);

interface ProviderListResp {
  providers?: ProviderLite[];
  currentId?: string;
}

export async function loadProviders(force = false): Promise<void> {
  if (!force && (providerState.value === "ready" || providerState.value === "loading")) return;
  providerState.value = "loading";
  try {
    const r = await invoke<ProviderListResp>("provider_list");
    providers.value = (r?.providers ?? []).map((p) => ({
      id: p.id,
      name: p.name,
      category: p.category,
      hasKey: p.hasKey,
      color: p.color,
    }));
    hostCurrentProvider.value = r?.currentId ?? "";
    providerState.value = "ready";
  } catch {
    // 老主机没这命令(404/不支持)/离线:按不支持处理,按钮隐藏
    providers.value = [];
    providerState.value = "unsupported";
  }
}

export async function loadSkills(force = false): Promise<void> {
  if (!force && (skillState.value === "ready" || skillState.value === "loading")) return;
  skillState.value = "loading";
  try {
    const list = await invoke<SkillLite[]>("list_skills");
    skills.value = (list ?? []).filter((s) => s.installed !== false);
    skillState.value = "ready";
  } catch {
    skills.value = [];
    skillState.value = "unsupported";
  }
}

export function toggleSkill(id: string): void {
  const cur = chosenSkillIds.value;
  chosenSkillIds.value = cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id];
}

/** 当前选中供应商的展示名(输入条 chip 用)。 */
export function chosenProviderName(): string {
  if (chosenProviderId.value === "auto") return "";
  return providers.value.find((p) => p.id === chosenProviderId.value)?.name ?? "";
}

/** 切主机/登出:清列表与选中,防上家主机的供应商 id 串到下家。 */
export function resetPickers(): void {
  providers.value = [];
  skills.value = [];
  providerState.value = "idle";
  skillState.value = "idle";
  hostCurrentProvider.value = "";
  chosenProviderId.value = "auto";
  chosenSkillIds.value = [];
  deepWork.value = false;
}
