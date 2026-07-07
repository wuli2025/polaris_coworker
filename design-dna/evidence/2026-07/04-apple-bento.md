---
id: 2026-07/04-apple-bento
source: deck.gallery/blog/apple-bento-grid-breakdown + medium bento breakdown + wearedevelopers bento css + figma bento template
collected: 2026-07-04
loop: 1
type: web
persona: bento-grid
score: 31/35
exemplar: false
---

# Apple / Linear 便当格（便当格大师的元教材）

## 一句话：它凭什么好
把「功能一大堆」摆成「秩序一整盘」——大小格用面积编码优先级，统一 gap 与圆角当缰绳，密而不乱，一屏讲完整个产品。

## 解构数据（实测）
- **网格**：桌面 4–6 等宽列 `minmax(0,1fr)`（或 12 列细分）；卡片 `grid-column/row span` 跨列做大小格；`grid-auto-flow: dense` 填洞。
- **gap**：统一 12–24px，最常见 **16px**——间距全局一致是「bento 感」核心。（← 印证 persona「16–20 全局唯一」，可放宽为 12–24 择一）
- **圆角**：12–24px，**Apple 统一 20px**。（← persona「16–20 全局唯一」微调为 12–24，Apple 20 为经典值）
- **格数上限**：一屏 **≤12 格**，超了塌成杂乱噪点。（← persona 缺此硬上限，必补）
- **面积编码**：1 大格(主菜) + 若干中格 + 边角小格；面积 = 优先级，不是全等大。
- **每格密度**：一格一个想法（1 特性 / 1 数字 / 1 图）。
- **画布底**：近黑 `#080808`（Apple keynote），每格自带产品图/单色/大数字；数字为主导排版元素，缩略图尺寸仍要可读。
- **响应式**：≥1024 用 4–6 列，768–1024 降 3 列，<768 塌单列。

## rubric 七维分
R1 4/5：深底浅字/彩格清晰。 R2 5/5：面积即优先级，主菜一眼锁定。 R3 4/5：近黑底 + 克制点彩，和谐但保守。
R4 5/5：统一 gap/圆角 + dense 网格，秩序满分。 R5 4/5：满盘密度高，节奏靠大小格变化。 R6 5/5：全局一致。 R7 4/5：清爽有记忆点但偏工具感。

## ★ 印证 / 挑战
- **印证** persona [A4 网格][A2 层级][A11 一致]：大小格戏法、满盘不乱术有据；gap/圆角全局唯一 ✓；一格一想法 ✓。
- **修正/补强** persona：① 圆角/gap 值放宽为 12–24 择一，标注 Apple 20px 为经典；② **补硬上限「一屏 ≤12 格」**（原缺）；③ 补 `grid-auto-flow:dense` 填洞与响应式塌列规则到实现映射；④ 深板画布近黑 `#080808` 收进色板。
