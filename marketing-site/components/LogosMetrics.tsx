import { logos, metrics } from "@/lib/data";
import Reveal from "./Reveal";

export default function LogosMetrics() {
  return (
    <section id="customers" className="section">
      <div className="container">
        <div className="logos">
          <p className="cap">已获得全球 12,800+ 团队的信赖</p>
          <div className="logo-row">
            {logos.map((name) => (
              <span key={name}>{name}</span>
            ))}
          </div>
        </div>

        <div className="metric-row">
          {metrics.map((m, i) => (
            <Reveal key={m.label} className="metric-cell" delay={i * 80}>
              <div className="v">{m.value}</div>
              <div className="l">{m.label}</div>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
