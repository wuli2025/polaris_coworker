import { features } from "@/lib/data";
import Reveal from "./Reveal";

export default function Features() {
  return (
    <section id="features" className="section">
      <div className="container">
        <div className="section-head">
          <span className="eyebrow">功能特性</span>
          <h2>把大模型能力，稳稳交付到生产</h2>
          <p>从推理性能到安全合规，云枢为规模化 AI 应用准备了完整的基础设施。</p>
        </div>

        <div className="feature-grid">
          {features.map((f, i) => (
            <Reveal
              key={f.title}
              className="feature-card"
              delay={(i % 3) * 80}
            >
              <div className="feature-ico">
                <svg viewBox="0 0 24 24" aria-hidden>
                  <path d={f.icon} />
                </svg>
              </div>
              <h3>{f.title}</h3>
              <p>{f.desc}</p>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
