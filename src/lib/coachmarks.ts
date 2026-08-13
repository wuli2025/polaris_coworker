/**
 * L2「点哪讲哪」词典。
 *
 * 一条只说一句、**15 字以内** —— 用户是在按钮旁边瞄一眼,不是在读说明书。
 * 用法:在目标按钮上写 `data-coach="index"`,别的什么都不用做;
 * 全局挂载的 Coach.vue 会在它首次露面时弹一次,点掉即永不再扰。
 */
export interface Coachmark {
  /** 功能名 */
  title: string;
  /** 一句话说清点了会怎样,15 字以内 */
  body: string;
}

export const COACHMARKS: Record<string, Coachmark> = {
  provider: { title: "API 供应商", body: "挑一家 AI,点选即切换" },
  index: { title: "建索引", body: "贴完标签,能按意思搜" },
  titles: { title: "AI 整理名称", body: "乱码文件名改回人话" },
  modes: { title: "模式 · 知识库", body: "先查你的资料再回答" },
  deep_search: { title: "深度搜索", body: "多来源交叉验证再汇总" },
  morning: { title: "每日晨报", body: "每天凌晨自己整理一遍" },
};

const KEY = "polaris.coach.v1";

function load(): Set<string> {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return new Set(JSON.parse(raw) as string[]);
  } catch {
    /* 存储不可用 → 当作全没看过,顶多多弹一次 */
  }
  return new Set();
}

let seen = load();

export function isSeen(key: string): boolean {
  return seen.has(key);
}

export function markSeen(key: string): void {
  if (seen.has(key)) return;
  seen.add(key);
  save();
}

/** 还有没有没看过的 —— Coach.vue 用它决定要不要继续扫 DOM(全看过就彻底停表)。 */
export function hasUnseen(): boolean {
  return Object.keys(COACHMARKS).some((k) => !seen.has(k));
}

/** 设置页「重看所有引导」用。 */
export function resetCoachmarks(): void {
  seen = new Set();
  save();
}

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify([...seen]));
  } catch {
    /* 同上 */
  }
}
