#!/usr/bin/env python3
# 压测语料生成器:中英混合 markdown + 植入可检索的"地面真值"事实,用于 recall 评测。
import os, random, json, sys
random.seed(1337)
base = sys.argv[1] if len(sys.argv) > 1 else "/root/Polaris/PolarisKB"
total = int(sys.argv[2]) if len(sys.argv) > 2 else 8000
roots = [base, base+"/wiki", base+"/raw", base+"/output", base+"/memory"]
for r in roots:
    os.makedirs(r, exist_ok=True)

cn_topics = ["退款政策","知识库检索","向量索引","营业时间","模型部署","数据安全","用户画像",
    "并发架构","缓存策略","多核盘点","语义聚类","重排服务","嵌入模型","全文倒排","知识图谱",
    "本体抽取","文件中心","对话体验","专家团队","自动化流程"]
cn_words = "公司 系统 数据 模型 检索 索引 向量 政策 退款 营业 时间 安全 用户 架构 缓存 并发 多核 盘点 语义 聚类 重排 嵌入 倒排 图谱 本体 抽取 文件 对话 专家 流程 知识 概念 算法 性能 优化 部署 容器 服务 接口 请求".split()
en_words = "system data model search index vector policy refund hours security user architecture cache concurrent retrieval embedding rerank ontology knowledge graph performance optimize deploy container service".split()

# 地面真值:每条 = (唯一答案串, 该串所在文件相对路径, 一组应能召回它的查询)
truth = []
n = 0
for i in range(total):
    r = random.choice(roots)
    topic = random.choice(cn_topics)
    nlines = random.randint(20, 200)
    lines = [f"# {topic} 文档{i}", ""]
    for _ in range(nlines):
        if random.random() < 0.6:
            lines.append("本文档讨论"+("".join(random.sample(cn_words, random.randint(3,8))))+"等问题。"+("".join(random.sample(cn_words, random.randint(2,6)))))
        else:
            lines.append(" ".join(random.sample(en_words, random.randint(4,10))))
    relpath = os.path.relpath(os.path.join(r, f"doc_{i:05d}.md"), base).replace("\\","/")
    # 植入唯一可检索事实(每隔若干文档一条),给定 query 应排第一
    if i % 137 == 0:
        token = f"ZX{i:05d}QW"  # 全局唯一的拉丁锚点
        lines.append(f"独家标识 {token} 对应营业时间早上9点至晚上9点。")
        truth.append({"q": token, "ans": token, "path": relpath, "kind": "latin-unique"})
    if i % 149 == 0:
        cnphrase = f"退款政策第{i}条特别说明"
        lines.append(f"{cnphrase}:大额退款需三个工作日审批。")
        truth.append({"q": cnphrase, "ans": cnphrase, "path": relpath, "kind": "cjk-phrase"})
    fn = os.path.join(r, f"doc_{i:05d}.md")
    open(fn, "w", encoding="utf-8").write("\n".join(lines))
    n += 1

json.dump(truth, open(os.path.join(base, "_ground_truth.json"), "w", encoding="utf-8"), ensure_ascii=False)
print(json.dumps({"generated": n, "roots": len(roots), "truth_count": len(truth)}, ensure_ascii=False))
