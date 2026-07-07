import { plans } from "@/lib/data";
import Reveal from "./Reveal";

function Check() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden>
      <path d="M9 16.2 4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4L9 16.2Z" />
    </svg>
  );
}

export default function Pricing() {
  return (
    <section id="pricing" className="section">
      <div className="container">
        <div className="section-head">
          <span className="eyebrow">定价</span>
          <h2>按需付费，透明可预期</h2>
          <p>从免费额度起步，随业务增长平滑扩展。所有套餐均无隐藏费用。</p>
        </div>

        <div className="price-grid">
          {plans.map((p, i) => (
            <Reveal
              key={p.name}
              delay={i * 90}
              className={`price-card${p.featured ? " featured" : ""}`}
            >
              {p.featured && <span className="price-tag">最受欢迎</span>}
              <h3>{p.name}</h3>
              <p className="tagline">{p.tagline}</p>
              <div className="price-amt">
                <span className="p">{p.price}</span>
                {p.period && <span className="per">{p.period}</span>}
              </div>
              <ul>
                {p.features.map((f) => (
                  <li key={f}>
                    <Check />
                    {f}
                  </li>
                ))}
              </ul>
              <a
                href="#"
                className={`btn ${p.featured ? "btn-primary" : "btn-ghost"}`}
              >
                {p.cta}
              </a>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
