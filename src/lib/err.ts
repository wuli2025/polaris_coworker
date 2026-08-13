/**
 * err.ts —— 统一的「把 catch 到的东西翻成人话」。
 *
 * 为什么需要它:Tauri 的 `Result<_, String>` 到 JS 是**一个字符串**,不是 Error。
 * 全仓原先在 catch 里对它取 `.message` —— 对字符串取那个属性恒为 `undefined`,
 * 于是内核辛辛苦苦写的「本机 P2P 身份还没就绪」「这台设备已被移出设备网」这类
 * 人话报错,一路走到 toast 变成一句「入网失败:undefined」。用户看到的是「又抽风了」,
 * 我们看到的是零线索。这个函数就是那道翻译闸,catch 里一律走它。
 */
export function errMsg(e: unknown): string {
  if (e == null) return "未知错误";
  if (typeof e === "string") return e.trim() || "未知错误";
  if (e instanceof Error) return e.message || String(e);
  if (typeof e === "object") {
    const o = e as Record<string, unknown>;
    // 后端 JSON 错误体({error}/{message}/{msg})与 axum 的 {error} 一并认。
    for (const k of ["error", "message", "msg", "detail"]) {
      const v = o[k];
      if (typeof v === "string" && v.trim()) return v.trim();
    }
    try {
      const s = JSON.stringify(e);
      if (s && s !== "{}") return s;
    } catch {
      /* 循环引用等:落到下面的兜底 */
    }
  }
  return String(e);
}

/** 带前缀的一行报错,`toast.error(errLine("入网失败", e))` 用。 */
export function errLine(prefix: string, e: unknown): string {
  return `${prefix}:${errMsg(e)}`;
}
