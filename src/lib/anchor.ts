/**
 * 把一个浮层贴到某个真实按钮旁边(引导气泡共用)。
 *
 * 规则:下方优先,下方放不下就翻到上方;左右夹在视口内,箭头始终指着按钮中心 ——
 * 引导必须**指着那颗键**,飘在角落的卡片说不清"到底点哪个"。
 */
export interface AnchorPos {
  top: number;
  left: number;
  /** 箭头相对浮层左边的偏移(px) */
  arrow: number;
  /** true = 浮层在按钮上方(箭头朝下) */
  above: boolean;
}

export function anchorTo(r: DOMRect, w: number, h: number, gap = 10): AnchorPos {
  const below = r.bottom + gap + h <= window.innerHeight - 8;
  const top = below ? r.bottom + gap : Math.max(8, r.top - gap - h);
  const want = r.left + r.width / 2 - w / 2;
  const left = Math.min(Math.max(8, want), Math.max(8, window.innerWidth - w - 8));
  return {
    top,
    left,
    arrow: Math.min(Math.max(14, r.left + r.width / 2 - left), w - 14),
    above: !below,
  };
}

/** 按钮是不是真的露在视口里(v-show 隐掉/尚未布局/被滚出去的都不算)。 */
export function onScreen(el: HTMLElement): boolean {
  const r = el.getBoundingClientRect();
  if (r.width < 8 || r.height < 8) return false;
  return r.top >= 0 && r.bottom <= window.innerHeight && r.left >= 0 && r.right <= window.innerWidth;
}
