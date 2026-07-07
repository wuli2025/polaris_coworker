import { marked } from "marked";

marked.setOptions({ breaks: true, gfm: true });

/** 极简 markdown → HTML。手机端不引重型 sanitizer;内容来自受信主机。 */
export function renderMd(src: string): string {
  try {
    return marked.parse(src) as string;
  } catch {
    return escapeHtml(src);
  }
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
