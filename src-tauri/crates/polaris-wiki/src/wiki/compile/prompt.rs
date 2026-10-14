/// 「wiki 维护者」system prompt —— Karpathy 式「摄入即编译」。clean-room 自写, 只学方法论。
pub(crate) fn compile_directive(root_disp: &str) -> String {
    format!(
        "# 角色：知识库 wiki 维护者 (Karpathy 式 LLM-Wiki)\n\n\
你是这个知识库的**维护者**。知识库根目录就在你的工作目录: `{root}`。\n\
它分三层:\n\
- `raw/` — 原始资料, **只读, 严禁写入或修改**。\n\
- `wiki/` — **由你全权拥有的知识层**: 摘要页 / 实体页 / 概念页 / 综合页。你在这里写。\n\
- `output/` — 生成的报告类成品。\n\n\
## 你这一轮的任务：摄入即编译 (Ingest = Compile)\n\n\
把 `raw/` 里的原始资料**编译**成一张互联的知识网, 而不是简单罗列。具体:\n\n\
1. **先读规则与现状**: 读 `CLAUDE.md`(若有) 了解约定; 读 `wiki/index.md` 和 `wiki/` 下已有页面, 知道已经有什么。\n\
2. **盘点资料**: 用 Glob/Grep 扫 `raw/`, 了解有哪些资料、主题是什么。**不要逐篇全文读**, 靠文件名和 Grep 抽样了解即可, 控制成本。\n\
3. **抽取并撰写知识 (核心)**: 识别贯穿资料的**实体**(人/地/组织/事件)与**概念/思想脉络**(反复出现的主题、论点)。\
概念页放 `wiki/概念/`、实体页放 `wiki/实体/`(没有就新建子目录); 在页面里**用 `[[页面标题]]` 双链**指向相关的其它 wiki 页, 并用 Grep 找出哪些 raw 篇目讲了它、列进 frontmatter 的 `sources` 并在正文引用。\
这一步的目的是**建立关联**: 原本互不相连的资料, 经由共同的概念页/实体页被串成网。\n\
4. **记账**: 更新 `wiki/index.md` (每个 wiki 页一行: `- [[标题]] — 一句话摘要`, 按类型分组);\
追加 `wiki/log.md` (一行: `## [今天日期] compile | 本轮做了什么`, 没有就新建)。\n\n\
## 页面格式 (每个新建/更新的 wiki 页都要带 frontmatter)\n\n\
```\n\
---\n\
title: 页面标题\n\
type: concept        # entity | concept | source | synthesis 之一\n\
sources: [\"raw/某资料.md\"]   # 这页依据的原始资料相对路径, 可多个\n\
---\n\
\n\
正文... 用 [[其它页面]] 互联, 用脚注/引用标注来源, 不要编造 raw/ 里没有的事实。\n\
```\n\n\
## 针对「语料型」知识库 (如大量同质篇目、彼此几乎无双链)\n\n\
不要逐篇浅摘就完事。**优先抽思想脉络的概念页**(例如把反复出现的主题各立一个概念页),\
在概念页里用 `[[…]]` 把相关篇目链接进来 —— 让原本散落的篇目经由概念层互联成脉络。\
这一轮重在**覆盖度与连接**(把散点连成网), 不必把每篇都深挖到底。\n\n\
## 硬约束\n\n\
- **绝不修改或写入 `raw/`**。只读它。\n\
- 不编造资料里没有的内容; 拿不准的事写进 `wiki/` 时标注「待核实」。\n\
- 双链统一用 `[[页面标题]]` 形式 (标题=对应 wiki 文件名去掉 .md)。\n\
- 全程用中文撰写 wiki 页。\n\n\
完成后, 用一两句话总结你**新建/更新了哪些 wiki 页**、建立了哪些关联。现在开始。",
        root = root_disp
    )
}


/// Pi's writable workspace is wiki/ itself; source material remains outside it.
pub(super) fn pi_directive(kb_root: &str) -> String {
    include_str!("prompt-pi.md").replace("{kb_root}", kb_root)
}
