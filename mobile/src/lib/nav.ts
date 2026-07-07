import { ref } from "vue";
import type { CapKey } from "./capabilities";

/** 当前主视图。与底部导航 + 触发面板共享。 */
export type Screen = CapKey | "settings";
export const screen = ref<Screen>("chat");

export function go(s: Screen): void {
  screen.value = s;
}
