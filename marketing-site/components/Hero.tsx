import AnimatedCounter from "./AnimatedCounter";

// Server Component. Receives the live count fetched on the server.
export default function Hero({ teamsServed }: { teamsServed: number }) {
  return (
    <section className="hero container">
      <div className="hero-grid">
        <div className="hero-copy">
          <span className="hero-badge">
            <span className="pill">新版本</span> 统一接入 40+ 主流大模型
          </span>

          <h1>
            让 AI 走向生产的
            <br />
            <span className="grad">最快一朵云</span>
          </h1>

          <p className="hero-sub">
            毫秒级推理、弹性算力与企业级安全，一套 OpenAI 兼容 API
            即可接入全部主流模型。从原型到规模化，都在云枢。
          </p>

          <div className="hero-cta">
            <a href="#pricing" className="btn btn-primary btn-lg">
              免费开始 →
            </a>
            <a href="#features" className="btn btn-ghost btn-lg">
              查看功能
            </a>
          </div>

          <div className="hero-count">
            <div className="avatars" aria-hidden>
              <span />
              <span />
              <span />
            </div>
            <span>
              已服务 <AnimatedCounter value={teamsServed} /> 团队
            </span>
          </div>
        </div>

        <div className="hero-visual">
          <div className="mock" role="img" aria-label="云枢控制台示意图">
            <div className="mock-bar">
              <i />
              <i />
              <i />
              <span>console.novacloud.ai</span>
            </div>
            <div className="mock-body">
              <div className="mock-metrics">
                <div className="mock-metric">
                  <div className="k">今日调用</div>
                  <div className="v">
                    1.24M <small>↑ 12%</small>
                  </div>
                </div>
                <div className="mock-metric">
                  <div className="k">P99 延迟</div>
                  <div className="v">
                    45ms <small>↓ 8%</small>
                  </div>
                </div>
                <div className="mock-metric">
                  <div className="k">成功率</div>
                  <div className="v">
                    99.99% <small>稳定</small>
                  </div>
                </div>
              </div>
              <div className="mock-chart" aria-hidden>
                {[38, 55, 44, 68, 52, 80, 62, 92, 74, 100].map((h, i) => (
                  <b
                    key={i}
                    style={{ height: `${h}%`, animationDelay: `${i * 60}ms` }}
                  />
                ))}
              </div>
              <div className="mock-log" aria-hidden>
                <span>POST /v1/chat/completions · 200 · 41ms</span>
                <span>model=nova-pro · tokens=512 · cached</span>
                <span>autoscale: 24 → 31 replicas</span>
              </div>
            </div>
          </div>

          <div className="float-card">
            <span className="ring" aria-hidden />
            <div>
              <div className="t">实时算力</div>
              <div className="b">2,480 GPU 在线</div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
