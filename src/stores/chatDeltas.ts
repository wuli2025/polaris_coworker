import type { Bubble } from "./chatTypes";

/** Batch token events while keeping tool/error/terminal events in order. */
export function createDeltaBuffer(ensureArr: (id: string) => Bubble[], intervalMs = 40) {
  if (!Number.isFinite(intervalMs) || intervalMs < 0) throw new Error("Invalid delta interval");
  const pending: Record<string, string> = {};
  let timer: ReturnType<typeof setTimeout> | null = null;
  function flush(id: string) {
    const text = pending[id];
    if (!text) return;
    delete pending[id];
    const arr = ensureArr(id);
    const last = arr[arr.length - 1];
    if (last && last.role === "assistant" && !last.err && !last.severity) last.text += text;
    else arr.push({ role: "assistant", text, at: Date.now() });
  }
  function enqueue(id: string, text: string) {
    pending[id] = (pending[id] ?? "") + text;
    if (!timer) timer = setTimeout(() => {
      timer = null;
      for (const key of Object.keys(pending)) flush(key);
    }, intervalMs);
  }
  return { flush, enqueue };
}
