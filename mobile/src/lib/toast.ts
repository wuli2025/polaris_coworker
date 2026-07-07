import { ref } from "vue";

export interface ToastItem {
  id: number;
  text: string;
  kind: "info" | "error" | "ok";
}
export const toasts = ref<ToastItem[]>([]);
let seq = 1;

export function toast(text: string, kind: ToastItem["kind"] = "info"): void {
  const id = seq++;
  toasts.value.push({ id, text, kind });
  setTimeout(() => {
    toasts.value = toasts.value.filter((t) => t.id !== id);
  }, 2800);
}
export function toastErr(e: unknown): void {
  toast(e instanceof Error ? e.message : String(e), "error");
}
