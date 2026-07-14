/**
 * 全局预览状态 —— 对话产物 chip / 文件列表都通过 openPreview(path) 拉起
 * App 级的 PreviewOverlay(App.vue 挂一次)。手机端不落文件,全部远程流式看。
 */
import { ref } from "vue";

export const previewPath = ref<string | null>(null);

export function openPreview(path: string): void {
  previewPath.value = path;
}
export function closePreview(): void {
  previewPath.value = null;
}
