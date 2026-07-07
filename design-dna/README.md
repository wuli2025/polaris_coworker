# Design-DNA · 设计师人格进化系统

> Polaris 的 PPT / 网页幻灯片 / 网站生成，从「选模板」升级为「选设计师」。
> 每个设计师是一份可进化的人格文件（design.md），底下压着一份不可动摇的纯美学地基。
> 本目录是这些人格的**训练场**：由 AI（长期由 Opus 4.8 执行）按 `loop/PROTOCOL.md` 持续采集网上的一流设计、解构成证据、提炼成人格升级，再实战校验后落盘。

## 目录地图

```
design-dna/
├── README.md              ← 你在这里（循环执行者每次先读它）
├── LOOP.html              ← 总规划书（人类阅读版：愿景/架构/循环全解）
├── foundation/            ★ 美学地基 —— 所有人格共同遵守，只有人类点头才能改
│   ├── aesthetics.md      纯美学底层公理（对比/层级/留白/网格/色彩/字体/韵律/克制…）
│   └── rubric.md          评分卡（7 维 × 1–5 分 + 一票否决项），采集与实战校验都用它
├── personas/              ★ 设计师人格 —— 循环的进化对象，一人一文件
│   ├── _TEMPLATE.md       人格文件规范（新建人格照此结构）
│   │  · 经典翼 ·
│   ├── swiss-modernist.md         瑞士格大师「网格判官」· 瑞士网格理性派
│   ├── bauhaus-functionalist.md   包豪斯大师「三原色工头」· 包豪斯功能主义
│   ├── wabi-sabi-minimal.md       留白大师「留白住持」· 日式侘寂
│   ├── noir-luxe.md               黑金大师「五厘金匠」· 黑金高端商务
│   ├── editorial-humanist.md      杂志大师「深夜主编」· 杂志编辑人文派
│   ├── neo-brutalist.md           粗野大师「毛坯房诗人」· 新粗野主义
│   ├── keynote-tech.md            发布会大师「黑场魔术师」· 发布会科技派
│   ├── pedagogy-clarity.md        课件大师「认知减负师」· 课堂认知设计（课程 PPT 专属）
│   │  · 绚丽翼 ·
│   ├── aurora-holographic.md      幻彩大师「光谱驯兽师」· 全息幻彩
│   ├── cyberpunk-neon.md          赛博霓虹大师「夜城灯牌师」· 赛博霓虹
│   ├── vaporwave-y2k.md           蒸汽波大师「赛博乡愁贩子」· 蒸汽波/Y2K
│   ├── oriental-grandeur.md       国潮彩大师「矿物颜料掌柜」· 国潮华彩
│   ├── memphis-pop.md             波普糖大师「糖果建筑师」· 孟菲斯波普
│   ├── art-deco-gatsby.md         金闪闪大师「晚宴总管」· 装饰艺术
│   │  · 网感翼（互联网原生，科技官网正在用的流派）·
│   ├── mist-gradient.md           弥散光大师「氛围感大师」· 弥散渐变风（Stripe/Figma 式）
│   ├── bento-grid.md              便当格大师「格间收纳师」· Bento 网格风（Apple/Linear 式）
│   ├── glass-crisp.md             玻璃酥大师「毛玻璃点心师」· 玻璃拟态风（iOS/visionOS 式）
│   ├── dopamine-pop.md            多巴胺大师「快乐配色师」· 多巴胺风（高能量品牌式）
│   ├── clay-soft.md               粘土大师「软萌手办师」· 粘土拟态风（软 3D 式）
│   ├── dev-dark.md                极简黑大师「深夜工程师」· 开发者极简风（Linear/Vercel 式）
│   │  · 大众翼（按大众审美刚需反推的高频场景流派）·
│   ├── biz-blue.md                商务蓝大师「汇报室定海针」· 商务简约风（职场汇报第一刚需）
│   ├── gov-red.md                 党政红大师「庄重红管家」· 党政机关风（党建/政务/国企）
│   ├── xhs-life.md                小红书大师「种草氛围师」· 生活分享风（小红书/ins 图文）
│   └── doodle-hand.md             手绘涂鸦大师「铅笔顽童」· 手绘涂鸦风（白板课堂/Sketchnote）
├── evidence/              证据卡 —— 每采集一个一流案例，解构成一张卡（按 YYYY-MM 分月）
│   └── README.md          证据卡格式
└── loop/                  循环机器
    ├── PROTOCOL.md        ★ 循环协议（执行者的操作手册，一步不许跳）
    ├── sources.md         采集源清单 + 轮换规则
    └── journal/           循环日志（每跑一轮写一篇，接力棒）
```

## 三条铁律（执行者必须内化）

1. **双重锚定**：任何写进人格文件的规则，必须同时引用 ≥2 张证据卡 + ≥1 条 foundation 公理。
   只有证据没有公理 = 追时髦；只有公理没有证据 = 闭门造车。两个都要。
2. **地基不可自改**：`foundation/` 只能由人类批准修改。循环可以在 journal 里**提案**改地基，但不能直接动手。
3. **人格是刀不是仓库**：每个人格文件 ≤ 400 行。新规则挤进来，就淘汰最弱的旧规则（进一条、审一条）。
   人格的价值在于取舍鲜明，不在于面面俱到。

## 快速开始（循环执行者）

```
1. 读本文件 → 读 loop/PROTOCOL.md → 读 loop/journal/ 里最新一篇（接上一棒）
2. 按 PROTOCOL 跑完一轮（采集→解构→提炼→对抗评审→落盘→实战校验→写日志）
3. 一轮只服务一个人格、预算内收工。宁可小步快跑，不可一轮贪多。
```

## 与 App 的关系（当前阶段：P1 孵化期）

人格在本目录孵化进化；成熟（status: stable）后由人类把它接入
`src-tauri/src/templates/skills/polaris-deck-studio|polaris-web-studio`（skills.rs include_str 落盘），
前端「演示工坊 / 网站生成」增加「设计师」选项。路线图详见 `LOOP.html`。
