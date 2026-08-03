/**
 * 「打开××项目」口头指令的解析与匹配 —— 纯函数,零依赖(可被脚本逐条验收)。
 *
 * 与 appIntent.ts 是姊妹关系,但意图不同:
 *   appIntent  → 打开电脑上正在跑的**应用**(直投一张网页)
 *   projectIntent → 切到电脑上的**项目**(此后对话在该项目绑定的文件夹里干活)
 * 两者在 ChatScreen 里按「项目优先」串联:说了「项目/工程/仓库」二字的先归这里。
 */

export interface ProjectLike {
  id: string;
  name: string;
  /** 项目绑定的电脑目录(绝对路径)。用户常按文件夹名称呼项目,故也参与匹配。 */
  work_dir?: string | null;
}

export type ProjectIntent =
  | { kind: "list" }
  | { kind: "open"; name: string; explicit: boolean };

/** 「有哪些项目 / 看看项目 / 项目列表」——不点名,只想看清单。 */
const LIST_RE =
  /^(?:请|帮我|麻烦)?(?:打开|看看|查看|列出|列一下|有哪些|都有哪些|切换)?(?:一下)?(?:我的|电脑上的|电脑里的|电脑的)?(?:项目|工程|仓库)(?:列表|清单|们)?(?:吧|呗|谢谢|\?|？)?$/i;

/** 「打开××项目」——明说了「项目/工程/仓库」,一定是要切项目,不是开应用。 */
const OPEN_EXPLICIT_RE =
  /^(?:请|帮我|麻烦)?(?:打开|进入|切到|切换到|切换|去|开|open|switch|cd)\s*(?:一下)?\s*[「『《"']?(.+?)[」』》"']?\s*(?:这个|那个)?\s*(?:项目|工程|仓库|repo)\s*(?:吧|呗|谢谢)?$/i;

/** 「切到××」——没说「项目」二字。只有名字真能对上某个项目时才算数(见 ChatScreen)。 */
const OPEN_LOOSE_RE =
  /^(?:请|帮我|麻烦)?(?:切到|切换到|打开|进入)\s*(?:一下)?\s*[「『《"']?(.+?)[」』》"']?\s*(?:吧|呗|谢谢)?$/i;

/** 名字里带这些就不是点名,是整句话 —— 与 appIntent 同一条纪律:宁可漏,不可错。 */
function badName(name: string): boolean {
  return (
    !name ||
    name.length > 24 ||
    /[,。;!?、,;!?\s]{2}|[,。;!?,;!?]/.test(name)
  );
}

/**
 * 是不是一句「切项目」的口头指令。
 * 长句(>40 字)当正经对话放行;`explicit` = 明说了「项目/工程/仓库」——
 * 非 explicit 的要调用方确认名字能匹配上项目,否则应放行给大模型,绝不吞用户的话。
 */
export function parseProjectIntent(text: string): ProjectIntent | null {
  const t = text.trim();
  if (!t || t.length > 40) return null;
  if (LIST_RE.test(t)) return { kind: "list" };
  const m = t.match(OPEN_EXPLICIT_RE);
  if (m) {
    const name = (m[1] ?? "").trim();
    // 「打开项目」这种(名字被前缀吃空)已被 LIST_RE 接走,这里名字不合法就整句放行。
    if (badName(name)) return null;
    return { kind: "open", name, explicit: true };
  }
  const l = t.match(OPEN_LOOSE_RE);
  if (!l) return null;
  const name = (l[1] ?? "").trim();
  if (badName(name)) return null;
  return { kind: "open", name, explicit: false };
}

const norm = (s: string) => s.toLowerCase().replace(/[\s_-]+/g, "");
/** 工作目录的末段目录名(用户常按文件夹名叫项目:「打开 polaris-app」)。 */
function dirName(p?: string | null): string {
  if (!p) return "";
  const parts = p.replace(/[\\/]+$/, "").split(/[\\/]+/);
  return parts[parts.length - 1] ?? "";
}

/** 在电脑上的项目里找最像的:名字/目录名全等优先,再互相包含。 */
export function matchProject<T extends ProjectLike>(projects: T[], q: string): T | null {
  const n = norm(q);
  if (!n) return null;
  const exact = projects.find(
    (p) => norm(p.name) === n || (p.work_dir ? norm(dirName(p.work_dir)) === n : false)
  );
  if (exact) return exact;
  // 包含匹配:两字以上才允许(单字「a」谁都能沾边,那不叫点名)
  if (n.length < 2) return null;
  return (
    projects.find((p) => {
      const name = norm(p.name);
      const dir = norm(dirName(p.work_dir));
      return (
        (name && (name.includes(n) || n.includes(name))) ||
        (dir && (dir.includes(n) || n.includes(dir)))
      );
    }) ?? null
  );
}
