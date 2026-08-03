/**
 * 能力管控:哪些功能对手机端可见/可用 —— "彰显能力可隐藏管控"。
 *
 * 三层收敛(从严到宽取交集):
 *  1. 出厂默认(DEFAULT_CAPS):哪些能力默认亮在手机壳。
 *  2. 角色闸:owner 看全部;普通 member 隐藏管理类(admin/合并强推等)。
 *  3. 本地覆盖(localStorage):用户在「设置」里手动关掉的能力。
 *
 * 未列出的能力一律隐藏(白名单),避免桌面重功能在手机上误触。
 */
import { ref } from "vue";
import { user } from "./auth";

export type CapKey =
  | "chat" // 聊天(核心,不可关)
  | "files" // 文件中心:浏览/上传/下载
  | "projects" // 协作项目 & 任务卡
  | "voice" // 语音输入
  | "skills" // 技能/工作流触发
  | "kb" // 知识库检索
  | "admin"; // 管理(成员/设备/强推)—— 仅 owner

export interface CapMeta {
  key: CapKey;
  label: string;
  icon: string; // emoji 占位,后续可换 SVG
  desc: string;
  ownerOnly?: boolean;
  locked?: boolean; // 不可被用户关闭
}

export const CAP_CATALOG: CapMeta[] = [
  { key: "chat", label: "对话", icon: "💬", desc: "与中控平台对话、下发工作", locked: true },
  { key: "files", label: "文件", icon: "📁", desc: "浏览 / 上传 / 下载远端文件" },
  { key: "projects", label: "项目", icon: "🗂️", desc: "协作项目与任务卡" },
  { key: "skills", label: "技能", icon: "⚡", desc: "触发技能与工作流" },
  { key: "kb", label: "知识库", icon: "📚", desc: "检索团队知识库" },
  { key: "voice", label: "语音", icon: "🎙️", desc: "按住说话,主机侧走火山识别转文字" },
  { key: "admin", label: "管理", icon: "🛡️", desc: "成员 / 设备 / 权限", ownerOnly: true },
];

const DEFAULT_ON: Record<CapKey, boolean> = {
  chat: true,
  files: true,
  projects: true,
  skills: true,
  kb: true,
  // 语音输入原先默认关(那时手机端根本没有能落地的转写后端)。现在录音在手机、
  // 识别在主机(火山),整条链路通了 → 默认开。用户仍可在设置里关掉。
  voice: true,
  admin: true,
};

const OVERRIDE_KEY = "polaris.m.caps.override";

function loadOverride(): Partial<Record<CapKey, boolean>> {
  try {
    return JSON.parse(localStorage.getItem(OVERRIDE_KEY) ?? "{}");
  } catch {
    return {};
  }
}

/** 响应式覆盖表:改动后 UI 立即反映。 */
export const capOverride = ref<Partial<Record<CapKey, boolean>>>(loadOverride());

export function setCap(key: CapKey, on: boolean): void {
  const meta = CAP_CATALOG.find((c) => c.key === key);
  if (meta?.locked) return; // 核心能力不可关
  capOverride.value = { ...capOverride.value, [key]: on };
  localStorage.setItem(OVERRIDE_KEY, JSON.stringify(capOverride.value));
}

/** 某能力当前是否可用(角色闸 ∩ 默认 ∩ 本地覆盖)。
 *  读的是响应式 user + capOverride,在 computed 里调用会自动联动。 */
export function hasCap(key: CapKey): boolean {
  const meta = CAP_CATALOG.find((c) => c.key === key);
  if (!meta) return false;
  if (meta.locked) return true;
  const isOwner = user.value?.role === "owner";
  if (meta.ownerOnly && !isOwner) return false;
  const ov = capOverride.value[key];
  if (ov !== undefined) return ov;
  return DEFAULT_ON[key];
}

/** 供设置页列出可切换的能力(排除锁定 + 无权限的)。响应式同 hasCap。 */
export function toggleableCaps(): CapMeta[] {
  const isOwner = user.value?.role === "owner";
  return CAP_CATALOG.filter((c) => !c.locked && (!c.ownerOnly || isOwner));
}
