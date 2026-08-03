/**
 * 「打开××」口头指令的解析与匹配 —— 纯函数,零依赖。
 *
 * 单独成文件是为了可测:apps.ts 牵着 net/nav(Capacitor 环境),在 node 里跑不起来;
 * 这里只有正则和字符串,`tsc` 编译后可直接被脚本调用逐条验收。
 */

export interface AppLike {
  slug: string;
  name: string;
}

export type OpenIntent =
  | { kind: "list" }
  | { kind: "open"; name: string; explicit: boolean };

/** 「打开/看看应用(列表)」——没点名,只想进应用页。 */
const LIST_RE =
  /^(?:请|帮我|麻烦)?(?:打开|看看|进入|切到)(?:一下)?(?:我的)?(?:应用|app)s?(?:列表|中心|页)?(?:吧|呗)?$/i;
/** 「打开××(应用)」——点名开某个应用。引号书名号剥掉;`explicit` = 明说了「应用」二字。 */
const OPEN_RE =
  /^(?:请|帮我|麻烦)?(?:打开|进入|启动|运行|open|launch|start)\s*(?:一下)?\s*[「『《"']?(.+?)[」』》"']?\s*(?:这个)?(应用|app)?\s*(?:吧|呗|谢谢)?$/i;

/**
 * 是不是一句「开应用」的口头指令。
 * 宁可漏、不可错:长句(>40 字)当正经对话放行;解析出的名字匹配不上已发布应用时,
 * 除非用户明说了「应用」,否则也放行给大模型 —— 绝不把用户的话吞掉。
 */
export function parseOpenIntent(text: string): OpenIntent | null {
  const t = text.trim();
  if (!t || t.length > 40) return null;
  if (LIST_RE.test(t)) return { kind: "list" };
  const m = t.match(OPEN_RE);
  if (!m) return null;
  const name = (m[1] ?? "").trim();
  // 应用名不会带标点、也不会二十多个字 —— 带了就是整句话,别当点名。
  if (!name || name.length > 20 || /[,。;!?、,;!?\s]{2}|[,。;!?,;!?]/.test(name)) return null;
  return { kind: "open", name, explicit: !!m[2] };
}

const norm = (s: string) => s.toLowerCase().replace(/\s+/g, "");

/** 在已发布应用里找最像的:全等(名字或 slug)优先,再互相包含。 */
export function matchApp<T extends AppLike>(apps: T[], q: string): T | null {
  const n = norm(q);
  if (!n) return null;
  return (
    apps.find((a) => norm(a.name) === n || a.slug === n) ??
    apps.find(
      (a) =>
        norm(a.name).includes(n) ||
        n.includes(norm(a.name)) ||
        a.slug.includes(n) ||
        n.includes(a.slug)
    ) ??
    null
  );
}
