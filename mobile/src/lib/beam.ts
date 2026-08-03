/**
 * 隔空同屏（手机侧）—— 把主机上的文件包装成一页自包含网页，手机与电脑同看一页。
 *
 * 链路：手机 POST /api/invoke(beam_send) → 主机广播总线 → 电脑 UI（tauri 事件桥）
 *      与其它手机（/ws）同时收到 `beam:sync`。整条通道复用已有的 iroh 隧道
 *      （hosts.ts probeEntry 会优先起 P2P 打洞，失败自动 relay 兜底），
 *      **因此不要求手机和电脑在同一 WiFi** —— 4G 也能投。
 *
 * 两端渲染的是**同一份 HTML**（beam_pack 的产物），所以滚动比例对得上、观感一致；
 * 页面塞进不给 allow-same-origin 的 sandbox iframe，页内脚本够不着主机接口。
 */
import { invoke, listen } from "./net";

/** 页内同屏代理脚本的固定标记（与 src-tauri/src/beam.rs 的 AGENT_JS 对齐）。 */
export const AGENT = "polaris-beam";

export interface BeamDoc {
  path: string;
  name: string;
  /** md | text | code | image | audio | video | html | svg | pdf | other */
  kind: string;
  html: string;
  bytes: number;
  htmlBytes: number;
  inlined: number;
  skipped: number;
  truncated: boolean;
}

export interface BeamMsg {
  act: "open" | "close" | "scroll" | "mark" | "zoom" | "ping" | "pong";
  from?: string;
  path?: string;
  name?: string;
  r?: number;
  /** mark: rx=视口宽度比例, ry=内容高度比例（两端版式不同也指同一段内容） */
  rx?: number;
  ry?: number;
  z?: number;
  ts?: number;
}

/** 本机标识：总线会把自己发的消息原样广播回来，不按 id 过滤就会「自己跟自己滚」。 */
export const ME: string = (() => {
  const K = "polaris.m.beam.id";
  let v = localStorage.getItem(K);
  if (!v) {
    v = `手机-${Math.random().toString(36).slice(2, 6)}`;
    localStorage.setItem(K, v);
  }
  return v;
})();

/** 把文件包装成一页自包含网页（图片/样式内联成 data URI）。 */
export function pack(path: string): Promise<BeamDoc> {
  return invoke<BeamDoc>("beam_pack", { path });
}

/** 导出成独立 .html 落在主机上，返回绝对路径。 */
export function exportPage(path: string): Promise<string> {
  return invoke<string>("beam_export", { path });
}

/** 发一条同步消息。自动补 from，失败静默（同步丢一帧不该弹错误框打断阅读）。 */
export function send(msg: BeamMsg): Promise<{ delivered: boolean } | null> {
  return invoke<{ delivered: boolean }>("beam_send", {
    msg: { ...msg, from: msg.from ?? ME },
  }).catch(() => null);
}

/** 订阅同屏消息；返回退订函数。 */
export function on(cb: (m: BeamMsg) => void): () => void {
  return listen<BeamMsg>("beam:sync", cb);
}
