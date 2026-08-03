/**
 * 应用直投（手机侧）—— 把电脑当服务器，手机只跑一个空壳网页端。
 *
 * 主机把它本机在跑的 HTTP 应用（开发服务器 / 带内置后端的桌面应用）发布出来，
 * 手机拿一张**只对那个应用有效**的会话票据 `/x/{sid}/`，在全屏 iframe 里直接用。
 * 业务逻辑、数据库、模型推理全在电脑上跑，手机上只有 DOM。
 *
 * 为什么票据在路径里而不是 cookie：手机壳是 Capacitor WebView，页面源是
 * `http://localhost`，而主机经隧道挂在 `http://127.0.0.1:<隧道口>` —— iframe 里算
 * **第三方**，安卓 WebView 默认拦第三方 cookie，纯 cookie 方案在 App 内必挂。
 * 路径票据 + 服务端 Referer 兜底才能让绝对路径的子资源（/assets/x.js）也认回来。
 */
import { ref } from "vue";
import { invoke, getBase } from "./net";
import { go } from "./nav";

export interface PubApp {
  slug: string;
  name: string;
  port: number;
  path: string;
  note: string;
}

/** 主机发布了哪些本机应用。 */
export function list(): Promise<PubApp[]> {
  return invoke<PubApp[]>("app_pub_list");
}

/** 开一张会话票据，返回可直接塞进 iframe 的**绝对** URL。 */
export async function open(slug: string): Promise<string> {
  const r = await invoke<{ sid: string; url: string }>("app_open", { slug });
  return getBase() + r.url;
}

// ────────────── 自然语言开应用(聊天里说「打开××」直接拉起,不劳驾大模型) ──────────────

/** 聊天里点名要打开的应用:切到「应用」页后由它在列表就绪时自动拉起。 */
export const pendingSlug = ref<string | null>(null);

export function requestOpen(slug: string): void {
  pendingSlug.value = slug;
  go("apps");
}

// 解析与匹配是纯函数,放在零依赖的 appIntent.ts 里(那边不牵 net/nav,可被脚本实测)。
export { parseOpenIntent, matchApp, type OpenIntent } from "./appIntent";
