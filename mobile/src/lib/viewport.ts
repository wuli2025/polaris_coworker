/**
 * 键盘 / 安全区适配 —— 让输入卡永远浮在输入法之上。
 *
 * 两条来源, 原生优先:
 *  ① **安卓壳(主路)**:MainActivity 的 WindowInsets 监听把 systemBars+cutout 和 ime
 *     三个值内联写到 `<html>` 的 `--safe-top/--safe-bottom/--kb`, 并派发 `polaris:insets`。
 *     这是唯一准的路 —— Android 15 强制 edge-to-edge 后窗口不再随输入法 resize,
 *     visualViewport 也就不会变, 纯前端根本感知不到键盘。
 *  ② **浏览器兜底(server flavor / 调试)**:没有原生注入时用 visualViewport 反推键盘高度。
 *
 * 对外只有两件事:`initViewport()` 装一次, `onKeyboard(cb)` 订阅键盘高度变化
 * (聊天页据此把消息流滚到底, 免得刚发的消息被键盘顶出视野)。
 */

const cbs = new Set<(kb: number) => void>();
let current = 0;

/** 订阅键盘高度(CSS px, 0 = 收起);返回退订函数。 */
export function onKeyboard(cb: (kb: number) => void): () => void {
  cbs.add(cb);
  return () => cbs.delete(cb);
}

/** 当前键盘高度(CSS px)。 */
export function keyboardHeight(): number {
  return current;
}

function emit(kb: number): void {
  const v = Math.max(0, Math.round(kb));
  if (v === current) return;
  current = v;
  for (const cb of cbs) cb(v);
  if (v > 0) revealFocused();
}

/**
 * 键盘弹起时把当前聚焦的输入框滚进可视区。
 *
 * 浏览器本来会自己干这件事 —— 但前提是窗口随输入法 resize。安卓 15 强制 edge-to-edge 后
 * 窗口尺寸压根不变,浏览器**不知道**下半屏被键盘盖住了,于是什么也不做。
 * 聊天页靠 `--kb` 把输入坞整体抬起来解决;而登录页/设置页那种「长表单里某个字段」
 * 只能靠这里补一脚滚动,否则永远是「点了输入框,字打在看不见的地方」。
 */
function revealFocused(): void {
  const el = document.activeElement as HTMLElement | null;
  if (!el) return;
  const tag = el.tagName;
  if (tag !== "INPUT" && tag !== "TEXTAREA" && !el.isContentEditable) return;
  // 等一帧:此刻 --kb 刚写进去,布局还没重排完,现在量位置会量到旧值。
  requestAnimationFrame(() => {
    try {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
    } catch {
      el.scrollIntoView();
    }
  });
}

declare global {
  interface Window {
    /** 原生 MainActivity 注入 insets 后置 1;有它就不跑 visualViewport 兜底。 */
    __polarisNativeInsets?: number;
  }
}

export function initViewport(): void {
  if (typeof window === "undefined") return;

  // ① 原生注入:CSS 变量原生已经写好了, 这里只把高度转成事件广播出去。
  window.addEventListener("polaris:insets", (e) => {
    const d = (e as CustomEvent<{ kb?: number }>).detail;
    emit(typeof d?.kb === "number" ? d.kb : 0);
  });

  // ② 浏览器兜底:visualViewport 缩小/上移的差值即键盘占的高度。
  //    原生已接管时直接不装, 免得两路互相打架(原生 0 / 兜底非 0 会抖)。
  const vv = window.visualViewport;
  if (!vv) return;
  const sync = () => {
    if (window.__polarisNativeInsets) return;
    const covered = window.innerHeight - (vv.height + vv.offsetTop);
    // 20px 以下当作地址栏收放之类的噪声, 不认成键盘。
    const kb = covered > 20 ? covered : 0;
    document.documentElement.style.setProperty("--kb", `${Math.round(kb)}px`);
    emit(kb);
  };
  vv.addEventListener("resize", sync);
  vv.addEventListener("scroll", sync);
  sync();
}
