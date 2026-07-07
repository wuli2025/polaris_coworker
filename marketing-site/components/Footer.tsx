const COLS = [
  {
    h: "产品",
    links: ["推理 API", "模型库", "微调平台", "控制台", "状态页"],
  },
  {
    h: "开发者",
    links: ["快速开始", "API 文档", "SDK", "更新日志", "系统状态"],
  },
  {
    h: "公司",
    links: ["关于我们", "客户案例", "招贤纳士", "博客", "联系销售"],
  },
];

function Social({ label, d }: { label: string; d: string }) {
  return (
    <a href="#" aria-label={label}>
      <svg viewBox="0 0 24 24" aria-hidden>
        <path d={d} />
      </svg>
    </a>
  );
}

export default function Footer() {
  return (
    <footer className="footer">
      <div className="container">
        <div className="footer-grid">
          <div className="footer-brand">
            <a href="#top" className="brand">
              <span className="dot" aria-hidden />
              云枢<span className="en">NovaCloud</span>
            </a>
            <p>
              面向生产的 AI 云平台。让每一个团队都能以更低的成本、更高的可靠性，
              将大模型能力交付给用户。
            </p>
          </div>

          {COLS.map((c) => (
            <div key={c.h} className="footer-col">
              <h4>{c.h}</h4>
              {c.links.map((l) => (
                <a key={l} href="#">
                  {l}
                </a>
              ))}
            </div>
          ))}
        </div>

        <div className="footer-bottom">
          <p>© {new Date().getFullYear()} 云枢 NovaCloud. 保留所有权利。</p>
          <div className="social">
            <Social
              label="GitHub"
              d="M12 2A10 10 0 0 0 8.8 21.5c.5.1.7-.2.7-.5v-1.7c-2.8.6-3.4-1.3-3.4-1.3-.5-1.2-1.1-1.5-1.1-1.5-.9-.6.1-.6.1-.6 1 .1 1.5 1 1.5 1 .9 1.6 2.4 1.1 3 .9.1-.7.4-1.1.6-1.4-2.2-.3-4.6-1.1-4.6-5 0-1.1.4-2 1-2.7-.1-.3-.4-1.3.1-2.7 0 0 .8-.3 2.7 1a9.4 9.4 0 0 1 5 0c1.9-1.3 2.7-1 2.7-1 .5 1.4.2 2.4.1 2.7.6.7 1 1.6 1 2.7 0 3.9-2.4 4.7-4.6 5 .4.3.7.9.7 1.9v2.8c0 .3.2.6.7.5A10 10 0 0 0 12 2Z"
            />
            <Social
              label="X"
              d="M18.9 2H22l-7.3 8.3L23 22h-6.8l-5-6.6L5.5 22H2.4l7.7-8.8L1.5 2h7l4.5 5.9L18.9 2Zm-2.4 18h1.7L7.2 3.8H5.4L16.5 20Z"
            />
            <Social
              label="微信"
              d="M8.5 4C4.9 4 2 6.4 2 9.4c0 1.7.9 3.2 2.4 4.2l-.6 1.8 2.1-1.1c.7.2 1.4.3 2.1.3h.5a4.6 4.6 0 0 1-.2-1.4c0-2.9 2.7-5.2 6.1-5.2h.4C14.2 5.7 11.6 4 8.5 4Zm-2.3 3a.9.9 0 1 1 0 1.8.9.9 0 0 1 0-1.8Zm4.6 0a.9.9 0 1 1 0 1.8.9.9 0 0 1 0-1.8ZM22 13.2c0-2.5-2.4-4.6-5.4-4.6s-5.4 2-5.4 4.6 2.4 4.6 5.4 4.6c.6 0 1.2-.1 1.8-.3l1.7.9-.5-1.5c1.4-.8 2.4-2.2 2.4-3.7Zm-7-1.1a.7.7 0 1 1 0 1.4.7.7 0 0 1 0-1.4Zm3.4 0a.7.7 0 1 1 0 1.4.7.7 0 0 1 0-1.4Z"
            />
          </div>
        </div>
      </div>
    </footer>
  );
}
