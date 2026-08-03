import { marked } from "marked";

marked.setOptions({ breaks: true, gfm: true });

/**
 * 极简 markdown → HTML。手机端不引重型 sanitizer;内容来自受信主机。
 *
 * 额外做一件事:**把正文里出现的文件路径变成可点的远程预览入口**。
 * 助手回复里大量文件只是顺口提一句(「已写到 ~/Polaris/projects/x/报告.md」),
 * 并不都会走 artifact 事件进 m.artifacts —— 只认 artifacts 的话这些文件在手机上
 * 就是一串死字。这里统一贴上 `data-path`,由 ChatScreen 的事件代理拉起 PreviewOverlay,
 * 文件本体仍在主机、手机只是远程看(/api/file 那头还有白名单闸把关)。
 */
/**
 * 渲染缓存。流式期间每 40ms 一个 delta 就把**整个消息列表**重渲一遍,
 * 每条都跑一次 marked + DOMParser + 全树遍历 = O(消息数 × 帧数),长对话必掉帧。
 * 已定稿的消息文本不再变,按文本缓存即可命中;只有正在流的那条每次是新键(本就得重算)。
 */
const RENDER_CACHE = new Map<string, string>();
const CACHE_CAP = 120;

export function renderMd(src: string): string {
  const hit = RENDER_CACHE.get(src);
  if (hit !== undefined) return hit;
  let out: string;
  try {
    const html = marked.parse(src) as string;
    try {
      out = linkifyPaths(html);
    } catch {
      out = html; // 路径识别失败绝不能把正文搞丢
    }
  } catch {
    out = escapeHtml(src);
  }
  // Map 保持插入序 → 首个键就是最老的,超额删一个即可(够用的 LRU 近似)。
  if (RENDER_CACHE.size >= CACHE_CAP) {
    const oldest = RENDER_CACHE.keys().next().value;
    if (oldest !== undefined) RENDER_CACHE.delete(oldest);
  }
  RENDER_CACHE.set(src, out);
  return out;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/** 认得出的文件后缀 —— 白名单而非「任意 .xxx」,否则 v1.2、3.5 这类都会被当文件。 */
const EXT =
  "html?|svg|pdf|png|jpe?g|gif|webp|bmp|ico|mp4|mov|webm|m4v|mp3|wav|m4a|ogg|flac|" +
  "md|markdown|txt|log|json|jsonl|csv|tsv|ya?ml|toml|xml|ini|conf|env|" +
  "sh|ps1|bat|py|rs|go|java|kt|swift|rb|php|c|h|cc|cpp|hpp|sql|" +
  "ts|tsx|js|jsx|mjs|cjs|vue|css|scss|less|" +
  "docx?|xlsx?|pptx?|zip|tar|gz|tgz|7z|rar";

/**
 * 路径正则拆成两支 —— **这是为了不把中文正文切坏**,别合并回一条。
 *
 * 中文不写空格,助手回复里「已更新config.json」「详见部署文档.md」是常态。
 * 若路径片段一律允许汉字,`已更新config.json` 会被整串认成路径、半句话染成蓝药丸,
 * 点开必 404 —— 这是用户第一眼就会看到的错。所以:
 *
 *  ① **带显式前缀**(`~/`、`./`、`/`、`D:\`)的才允许中文段 —— 有前缀就说明作者
 *     真的在写路径,`~/Polaris/项目/报告.md` 这种要完整认出来;
 *  ② **没有前缀**的裸文件名/相对路径只认纯 ASCII —— 于是 `已更新config.json` 里
 *     汉字天然落在字符类外,匹配从 `c` 开始,正好得到 `config.json`;
 *     而 `详见部署文档.md` 一个 ASCII 名都凑不出 → 干脆不匹配(宁可漏,不可错)。
 */
const CJK_SEG = "[\\w\\u4e00-\\u9fa5.\\-+()\\[\\]@]";
const ASCII_SEG = "[A-Za-z0-9_.+\\-@]";
const PREFIX = "(?:[A-Za-z]:[\\\\/]|~[\\\\/]|\\.{1,2}[\\\\/]|/)";

const PATH_RE = new RegExp(
  "(?:" +
    // ① 有前缀:允许中文目录/文件名
    `${PREFIX}(?:${CJK_SEG}+[\\\\/])*${CJK_SEG}+\\.(?:${EXT})` +
    "|" +
    // ② 无前缀:纯 ASCII
    `(?:${ASCII_SEG}+[\\\\/])*${ASCII_SEG}+\\.(?:${EXT})` +
    ")\\b",
  "gi"
);

/** 匹配到的这一串自带前缀吗(决定它是①还是②那支)。 */
const HAS_PREFIX = new RegExp(`^${PREFIX}`);

/** 已经是网址的不碰:紧邻在前的若是未闭合的 URL,说明这段是 URL 的路径部分。 */
const URL_TAIL = /(?:https?|ftp|wss?):\/\/\S*$/i;

/** 看着像文件路径吗?过滤掉「句号 + 后缀」这种误伤(如 "见 readme.md" 是真的,"3.14" 不是)。 */
function looksLikePath(s: string): boolean {
  if (s.length < 3 || s.length > 400) return false;
  // 纯数字文件名(1.2、2026.07)基本都是版本号/日期,不认
  const name = s.split(/[\\/]/).pop() ?? s;
  if (/^\d+(\.\d+)*$/.test(name.replace(/\.[^.]+$/, ""))) return false;
  return true;
}

function makeAnchor(doc: Document, raw: string): HTMLElement {
  const a = doc.createElement("a");
  a.className = "fpath";
  a.setAttribute("data-path", raw);
  a.setAttribute("role", "button");
  a.textContent = raw;
  return a;
}

function linkifyPaths(html: string): string {
  if (typeof DOMParser === "undefined") return html;
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  const body = doc.body;

  // ① markdown 里写成链接、但 href 是本地路径的([报告](~/x/报告.md))→ 转成预览入口,
  //    保持 <a> 外形不变,只是换成 data-path(留着 href 会被 WebView 当外链跳走)。
  for (const a of Array.from(body.querySelectorAll("a[href]"))) {
    const href = a.getAttribute("href") ?? "";
    if (/^(https?:|mailto:|tel:|data:|#|\/\/)/i.test(href)) continue;
    if (!PATH_RE.test(href)) {
      PATH_RE.lastIndex = 0;
      continue;
    }
    PATH_RE.lastIndex = 0;
    a.removeAttribute("href");
    a.setAttribute("data-path", href);
    a.classList.add("fpath");
  }

  // ② 正文文本里裸写的路径。跳过 pre(整块代码里挨个变蓝太吵)与已经是链接的部分。
  const walker = doc.createTreeWalker(body, NodeFilter.SHOW_TEXT);
  const targets: Text[] = [];
  let node = walker.nextNode() as Text | null;
  while (node) {
    const el = node.parentElement;
    if (el && !el.closest("a, pre")) targets.push(node);
    node = walker.nextNode() as Text | null;
  }

  for (const t of targets) {
    const text = t.data;
    PATH_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    let cursor = 0;
    let frag: DocumentFragment | null = null;
    while ((m = PATH_RE.exec(text))) {
      const raw = m[0];
      if (!looksLikePath(raw) || URL_TAIL.test(text.slice(0, m.index))) continue;
      // 紧贴在分隔符右边的无前缀 ASCII 名 = 从一条中文相对路径(`报告/report.md`)里
      // 抠出来的半截,单独拿去预览必 404 —— 宁可不给点,也别给个死链接。
      if (!HAS_PREFIX.test(raw) && /[\\/]/.test(text[m.index - 1] ?? "")) continue;
      // 裸 `/` 开头的只有前面是干净边界时才算绝对路径。否则同样是从中间抠出来的半截:
      //   `报告/周报.md`          → 会从中间那个 `/` 起匹配出 `/周报.md`
      //   `https://x.com/a/b.png` → 会从第二个 `/` 起匹配出 `/x.com/a/b.png`
      // (后者 URL_TAIL 拦不住 —— 那时前缀只剩 `https:/`,还没凑齐 `://`。)
      if (raw.startsWith("/") && /[\w一-龥:/\\]/.test(text[m.index - 1] ?? "")) continue;
      frag ??= doc.createDocumentFragment();
      if (m.index > cursor) frag.appendChild(doc.createTextNode(text.slice(cursor, m.index)));
      frag.appendChild(makeAnchor(doc, raw));
      cursor = m.index + raw.length;
    }
    if (frag) {
      if (cursor < text.length) frag.appendChild(doc.createTextNode(text.slice(cursor)));
      t.parentNode?.replaceChild(frag, t);
    }
  }

  return body.innerHTML;
}
