// Server-side data layer. In production these would hit a real API/DB.
// Kept here so Server Components can `await` fresh numbers on each request.

export type Metric = { label: string; value: string };
export type Feature = { icon: string; title: string; desc: string };
export type Plan = {
  name: string;
  price: string;
  period: string;
  tagline: string;
  features: string[];
  cta: string;
  featured?: boolean;
};
export type Faq = { q: string; a: string };

// Simulates reading the live "teams served" counter from the backend.
// `no-store`-style: recomputed each request so the Hero counter is real-time.
export async function getTeamsServed(): Promise<number> {
  // e.g. const res = await fetch(process.env.API_URL + '/stats/teams', { cache: 'no-store' })
  const base = 12800;
  const drift = Math.floor((Date.now() / 3_600_000) % 240); // slow hourly drift
  return base + drift;
}

export async function getPlatformStatus(): Promise<{
  ok: boolean;
  uptime: string;
}> {
  return { ok: true, uptime: "99.99%" };
}

export const features: Feature[] = [
  {
    icon: "M13 2 3 14h7l-1 8 10-12h-7l1-8Z",
    title: "毫秒级推理",
    desc: "自研调度引擎与就近边缘节点，P99 延迟低至 45ms，为实时应用而生。",
  },
  {
    icon: "M12 2 2 7l10 5 10-5-10-5Zm0 12L2 9v6l10 5 10-5V9l-10 5Z",
    title: "弹性算力",
    desc: "秒级扩缩容至数千张 GPU，按用量计费，闲时零成本，峰值不排队。",
  },
  {
    icon: "M12 1 3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4Z",
    title: "企业级安全",
    desc: "SOC 2 与等保三级合规，数据默认加密，支持私有化与 VPC 隔离部署。",
  },
  {
    icon: "M4 4h16v12H5.17L4 17.17V4Zm0 16 4-4h12a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v16Z",
    title: "统一 API",
    desc: "一套 OpenAI 兼容接口接入全部主流模型，切换模型无需改代码。",
  },
  {
    icon: "M3 3v18h18v-2H5V3H3Zm4 12 4-4 3 3 5-6 1.4 1.4L14 17l-3-3-3 3-1-2Z",
    title: "可观测性",
    desc: "调用链、Token 用量、成本与质量指标一屏尽览，异常实时告警。",
  },
  {
    icon: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm1 15h-2v-6h2v6Zm0-8h-2V7h2v2Z",
    title: "专家支持",
    desc: "北京时间 08:00–20:00 工程师在线值守，企业版提供专属解决方案架构师。",
  },
];

export const metrics: Metric[] = [
  { label: "服务等级协议 (SLA)", value: "99.99%" },
  { label: "日均 API 调用", value: "3.2 亿次" },
  { label: "全球边缘节点", value: "180+" },
  { label: "P99 推理延迟", value: "45ms" },
];

export const logos: string[] = [
  "北极星科技",
  "云启资本",
  "极智制造",
  "启明医疗",
  "远景出行",
  "翎动传媒",
];

export const plans: Plan[] = [
  {
    name: "开发者",
    price: "¥0",
    period: "/月",
    tagline: "适合个人开发者与小型验证项目",
    features: ["每月 100 万 Token 额度", "全部开源模型", "社区支持", "标准速率限制"],
    cta: "免费开始",
  },
  {
    name: "团队",
    price: "¥1,999",
    period: "/月",
    tagline: "适合快速成长的产品团队",
    features: [
      "每月 1 亿 Token 额度",
      "全部主流商业模型",
      "工程师在线值守支持",
      "10 倍速率限制",
      "团队协作与用量看板",
    ],
    cta: "开始 14 天试用",
    featured: true,
  },
  {
    name: "企业",
    price: "定制",
    period: "",
    tagline: "适合对合规与规模有要求的组织",
    features: [
      "无限 Token 额度",
      "私有化 / VPC 部署",
      "专属架构师与 SLA",
      "SSO 与审计日志",
      "定制模型微调",
    ],
    cta: "联系销售",
  },
];

export const faqs: Faq[] = [
  {
    q: "如何开始使用？需要绑定信用卡吗？",
    a: "注册即可获得开发者版免费额度，无需绑定任何支付方式。当额度用尽或需要更高速率时，再升级到付费套餐即可。",
  },
  {
    q: "你们支持哪些大模型？",
    a: "我们通过统一的 OpenAI 兼容 API 提供数十种主流开源与商业模型，并持续接入最新版本。切换模型只需修改一个参数，无需改动业务代码。",
  },
  {
    q: "数据安全和隐私如何保障？",
    a: "平台已通过 SOC 2 与等保三级认证。默认对传输与存储数据加密，且不会将你的请求数据用于任何模型训练。企业版支持私有化与 VPC 隔离部署。",
  },
  {
    q: "计费方式是怎样的？",
    a: "开发者版免费；团队版为固定月费加超量按用量计费；企业版为定制合同。所有用量都可在控制台实时查看，绝无隐藏费用。",
  },
  {
    q: "能否随时升级或取消套餐？",
    a: "可以。套餐升级立即生效，降级或取消将在当前计费周期结束时生效，操作全部在控制台自助完成。",
  },
];
