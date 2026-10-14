import { useAppStore } from "./app";
import { useArtifactsStore } from "./artifacts";
import type { Bubble } from "./chat";

/** 对话框只展示用户能直接打开的常见成品格式(与后端 chat.rs DISPLAY_EXTS 同步);
 *  脚本/配置等中间产物不展示。带尾随 `/` 的是「应用文件夹」chip, 一律保留。
 *  这道前端过滤主要兜底**旧历史**: 白名单上线前落库的 marker 里还混着中间文件。 */
const DISPLAY_EXTS = new Set([
  "md", "markdown", "txt", "pdf", "doc", "docx", "ppt", "pptx", "xls", "xlsx", "csv",
  "html", "htm",
  "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif", "ico",
  "mp4", "mov", "webm", "mkv", "avi", "mp3", "wav", "m4a", "aac", "flac", "ogg",
  "zip",
]);
function isDisplayableArtifact(path: string): boolean {
  if (path.endsWith("/")) return true; // 应用文件夹
  const name = path.split("/").pop() || path;
  // 演示 spec 是「课件源稿」一等产物(右抽屉开成播放器、可导出 pptx),
  // 不能被 json 不在白名单这条通则误杀 —— 否则重启后历史里 spec chip 消失。
  if (/^polaris\.slides\.json$/i.test(name)) return true;
  const i = name.lastIndexOf(".");
  return i >= 0 && DISPLAY_EXTS.has(name.slice(i + 1).toLowerCase());
}

// 「强成品」= 本轮结束后值得自动弹进右抽屉预览的可打开件(Kimi 式:生成完直接看到成品)。
// 只认网页 / 演示 spec / pdf 这类「一眼即成品」的东西;零碎数据文件(json/csv/txt…)不自动弹,
// 免得打扰。优先级:演示 spec > 网页 > pdf(与 shared.ts 的预览排序同源,这里只取会弹的子集)。
function pickDeliverable(arts: string[]): string | undefined {
  const files = arts.filter((a) => !a.endsWith("/"));
  const rank = (p: string): number => {
    if (/polaris\.slides\.json$/i.test(p)) return 0;
    const n = p.split(/[\\/]/).pop() || p;
    const ext = n.slice(n.lastIndexOf(".") + 1).toLowerCase();
    if (ext === "html" || ext === "htm") return 1;
    if (ext === "pdf") return 2;
    return 99; // 其余不自动弹
  };
  let best: string | undefined;
  let bestRank = 99;
  for (const p of files) {
    const r = rank(p);
    if (r < bestRank) {
      bestRank = r;
      best = p;
    }
  }
  return bestRank < 99 ? best : undefined;
}

/** 解析正文里夹带的产物清单 marker，返回剥离 marker 后的纯文本 + 路径数组 */
export function parseArtifacts(content: string): {
  text: string;
  artifacts: string[];
} {
  const m = content.match(/<!--POLARIS_ARTIFACTS:(\[[\s\S]*?\])-->/);
  if (!m) return { text: content, artifacts: [] };
  let arr: string[] = [];
  try {
    arr = JSON.parse(m[1]);
  } catch {
    arr = [];
  }
  const text = content.replace(m[0], "").trimEnd();
  return { text, artifacts: arr.filter(isDisplayableArtifact) };
}

export function receiveArtifact(arr: Bubble[], cid: string, path?: string) {
  if (path) {
    let target: Bubble | undefined;
    for (let i = arr.length - 1; i >= 0; i--) {
      if (arr[i].role === "assistant" && !arr[i].err && !arr[i].severity) {
        target = arr[i];
        break;
      }
    }
    if (!target) {
      target = { role: "assistant", text: "", artifacts: [] };
      arr.push(target);
    }
    if (!target.artifacts) target.artifacts = [];
    if (!target.artifacts.includes(path)) {
      target.artifacts.push(path);
      // 豆包化:演示 spec 一落盘,右抽屉自动开成播放器(配合抽屉的宽容解析
      // 轮询逐页点亮),用户不必等生成结束、也不必自己点产物 chip。
      // push 去重保证一轮只触发一次。放在事件源头做(而非组件 watch):命令式、
      // 无响应性依赖,不会静默失灵。
      if (/polaris\.slides\.json$/i.test(path)) {
        try {
          const app = useAppStore();
          const arts = useArtifactsStore();
          // 抢焦点的分寸:①必须是用户正看的这条对话;②抽屉空着、或正看同一个
          // 文件 → 开;③抽屉停在**别条对话**的产物上 = 陈旧,必须让位(用户刚要
          // 的就是这份新课件,不能让他对着上一份发呆——这曾导致「导出」导出了
          // 上一条对话的旧课件);④同对话内用户特意开着别的文件 → 尊重,不抢。
          const cur = arts.current?.path;
          const stale = !!cur && !cur.replace(/\\/g, "/").includes(`/conversations/${cid}/`);
          if (app.currentConvId === cid && (!cur || cur === path || stale)) {
            app.drawerCollapsed = false;
            void arts.open(path);
          }
        } catch {
          /* 抽屉打不开也不能砸了流式处理 */
        }
      }
    }
  }
}

export function previewDeliverable(arr: Bubble[], cid: string) {
  const app = useAppStore();
  // Kimi 式:本轮结束,把「主成品」(网页/演示/pdf)自动弹进右抽屉预览 —— 用户不必
  // 生成完还得自己去点产物 chip。沿用演示 spec 那套抢焦点分寸:①必须是用户正看的这条
  // 对话;②抽屉空着、或停在别条对话的旧产物上(陈旧,让位)才弹;③同对话内用户特意
  // 开着别的文件 → 尊重不抢。slides 已在 artifact 事件即时弹过(cur===它、同对话非陈旧),
  // 这里的 `!cur || stale` 判断天然不会重复弹它。
  try {
    const arts2 = useArtifactsStore();
    let start = 0;
    for (let i = arr.length - 1; i >= 0; i--) {
      if (arr[i].role === "user") {
        start = i;
        break;
      }
    }
    const round: string[] = [];
    for (let i = start; i < arr.length; i++) {
      if (arr[i].role === "assistant" && arr[i].artifacts)
        round.push(...arr[i].artifacts!);
    }
    const deliverable = pickDeliverable(round);
    if (deliverable && app.currentConvId === cid) {
      const cur = arts2.current?.path;
      const stale =
        !!cur &&
        !cur.replace(/\\/g, "/").includes(`/conversations/${cid}/`);
      if (!cur || stale) {
        app.drawerCollapsed = false;
        void arts2.open(deliverable);
      }
    }
  } catch {
    /* 自动预览失败绝不能砸了终态处理 */
  }
}
