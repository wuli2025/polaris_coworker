---
id: 2026-07/03-stripe-linear-mist
source: kevinhufnagl.com/how-to-stripe-website-gradient-effect + linear.app/brand + fontofweb.com/tokens/linear.app + vercel.com/geist
collected: 2026-07-04
loop: 1
type: web
persona: mist-gradient
score: 32/35
exemplar: true
---

# Stripe / Linear 弥散渐变（弥散光大师的元教材）

## 一句话：它凭什么好
把颜色调成会缓慢流动的空气——深底上一层失焦极光，锋利白字浮在其上，清晰与朦胧对撞出「一定融过资」的高级感。

## 解构数据（实测 hex）
- **Stripe 光斑 4 色（逆向实测）**：`#6ec3f4`(浅蓝) · `#3a3aff`(深蓝) · `#ff61ab`(粉) · `#E63946`(红)——蓝紫粉红一路扫，不是纯邻近但连续过渡。
- **Linear 角向渐变色标**：`#08AEEA`→`#2AF598`→`#B5FFFC`→`#FF5ACD`→`#FFFFFF`（青→绿→浅蓝→粉→白）。
- **光斑机制**：3–4 层 radial-gradient，`at` 散点于 top-left/bottom-right/center；高 blur 半径消除 banding；玻璃卡 `backdrop-filter: blur(10–15px)` + `1px solid rgba(255,255,255,.18)` 把清晰内容从模糊背景「提」出来。
- **背景**：**旗舰是深色**——Stripe/Linear/Vercel 走近黑（Linear 营销站 `#010102` 带微蓝）；**Figma 是浅底 pastel 的例外变体**。（← 修正我 persona：原以浅底为主板，应改深底为旗舰、浅底为变体）
- **文字**：深底白/近白 `#ffffff`~`#f5f5f5`；渐变叠加区文字用蒙版保对比。
- **强调**：Linear 全站单一薰衣草紫 `#5e6ad2`，每屏仅 1 个主 CTA 用它。
- **字体**：Inter Display(标题)+Inter(正文)，`font-feature-settings:'cv01','ss03','zero'`；标题 32–48px、正文 ~16px。
- **渐变字边界**：只用于 logo/球体/极个别英雄词，正文 UI 一律纯色。

## rubric 七维分
R1 5/5：蒙版保住白字对比。 R2 4/5：光斑背景 + 单 CTA，焦点清晰。 R3 5/5：连续色相扫过，灰度后仍有明度层次。
R4 4/5：Inter 字阶规整，营销页网格略松。 R5 5/5：深色负空间留足，光有方向。 R6 4/5：单强调一致。 R7 5/5：一眼硅谷。

## ★ 印证 / 挑战
- **印证** persona [A3 留白][A5 色彩][A9 光]：空气感光斑、刀锋排版、清晰度对撞全部有据；blur 大半径、渐变字只给英雄词 ✓。
- **修正** persona：① **深底为旗舰主板**（原浅底优先，反了）；② 光斑「邻近 ≤2–3 色」放宽为「3–4 色连续扫色、单步过渡不断裂」（Stripe 实为 4 色）；③ 强调色补 Linear `#5e6ad2` 为参考、「每屏 1 个主 CTA」写进版式；④ 玻璃卡 backdrop-blur 10–15px 是把内容从雾里提出来的关键手法，应写进「清晰度对撞」。
- **交叉归属**：Linear/Vercel 数据同时喂养 极简黑（见 ev01）与 发布会 大师。
