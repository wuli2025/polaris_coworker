import { ref } from "vue";
import type { CapKey } from "./capabilities";

/** 当前主视图。与底部导航 + 触发面板共享。hosts = 主机选择(UU式设备列表)。 */
export type Screen = CapKey | "settings" | "hosts";
// 冷启动第一屏 = 主机选择(UU远程式):已登录也先看到设备列表,点一下再进对话。
export const screen = ref<Screen>("hosts");

export function go(s: Screen): void {
  screen.value = s;
}
