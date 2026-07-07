# Loop 0d · v3：网感翼扩编 + 全员改流派可爱名 · 2026-07-04

本轮性质：花名册重构（Fable 5 执行，按用户反馈）。规划书升 v3.0。**本表为最新花名册，覆盖 0c 的旧名单。**

## 用户反馈（后来者必读，这是命名与选派的宪法）
1. 不要只收美术史老流派——要**互联网原生流派**：科技公司官网此刻正在用的设计语言（Stripe 弥散光、Apple Bento、Linear 极简黑……）。
2. 名字不要生僻专业人名——用**流派可爱名**：从流派关键词直接变出 2–4 字名字（国潮彩、赛博猕猴、便当格），大众一看名字就知道是什么风格。已写进 _TEMPLATE.md。

## 最新花名册（20 位）
| 翼 | 人名 | 名号 | id |
|---|---|---|---|
| 经典 | 瑞士格 | 网格判官 | swiss-modernist |
| 经典 | 包豪宝 | 三原色工头 | bauhaus-functionalist |
| 经典 | 留白酱 | 留白住持 | wabi-sabi-minimal |
| 经典 | 黑金爷 | 五厘金匠 | noir-luxe |
| 经典 | 杂志咖 | 深夜主编 | editorial-humanist |
| 经典 | 粗野仔 | 毛坯房诗人 | neo-brutalist |
| 经典 | 大场面 | 黑场魔术师 | keynote-tech |
| 经典 | 明白老师 | 认知减负师 | pedagogy-clarity |
| 绚丽 | 幻彩虹 | 光谱驯兽师 | aurora-holographic |
| 绚丽 | 赛博猕猴 | 夜城灯牌师 | cyberpunk-neon |
| 绚丽 | 蒸汽波波 | 赛博乡愁贩子 | vaporwave-y2k |
| 绚丽 | 国潮彩 | 矿物颜料掌柜 | oriental-grandeur |
| 绚丽 | 波普糖 | 糖果建筑师 | memphis-pop |
| 绚丽 | 金闪闪 | 晚宴总管 | art-deco-gatsby |
| 网感 | 弥散光 | 氛围感大师 | mist-gradient |
| 网感 | 便当格 | 格间收纳师 | bento-grid |
| 网感 | 玻璃酥 | 毛玻璃点心师 | glass-crisp |
| 网感 | 多巴胺 | 快乐配色师 | dopamine-pop |
| 网感 | 粘土泥 | 软萌手办师 | clay-soft |
| 网感 | 极简黑 | 深夜工程师 | dev-dark |

## 网感翼的量化推演值（采集时优先验证/修正）
- 弥散光：光斑透明度 ≤40%、blur ≥100px、色相 ≤3 个（去 Stripe/Figma 官网实测）
- 便当格：格间距/圆角全局唯一、彩色格 ≤1/3（去 Apple 产品页/Linear 实测）
- 玻璃酥：三层酥皮配方（blur 16–24px + 顶高光 + 6% 内衬）、同视线一层玻璃（对照 iOS/visionOS）
- 多巴胺：三件套撞色（一亮一深一奶）、大面积亮色饱和 ≤75%
- 粘土泥：双影方向统一（外影右下/内高光左上）、饱和 ≤60%
- 极简黑：细边 8–12% 透明度、强调色占比 2%、字重 ≤600（去 Linear/Vercel 扒 CSS 验证）

## 人格边界速记（防趋同）
波普糖 vs 多巴胺 = 黑描边复古几何 vs 无描边现代果冻；大场面 vs 极简黑 = 发布会煽情黑 vs 控制台禁欲黑；
弥散光 vs 幻彩虹 = 素字+光雾背景 vs 渐变字+光带主角；粘土泥 vs 多巴胺 = 低饱和软 3D vs 高饱和平面。

## 下一棒（改派）
- Loop 1：**弥散光**（网感翼先行——采集源就是一线官网 CSS，证据最新鲜、验证成本最低）。
  侧重：WebFetch Stripe/Figma/Notion 营销页，实测光斑参数与字阶；奇数轮 → deck 样张。
- Loop 2：便当格（Apple 产品页 + Linear）；Loop 3：极简黑（Linear/Vercel/GitHub 扒 CSS token）。
- 原 keynote-tech（大场面）顺延到 Loop 4。
