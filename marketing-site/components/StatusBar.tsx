"use client";

import { useEffect, useState } from "react";

function greetingFor(hour: number): string {
  if (hour >= 5 && hour < 12) return "早上好";
  if (hour >= 12 && hour < 18) return "下午好";
  if (hour >= 18 && hour < 23) return "晚上好";
  return "夜深了";
}

export default function StatusBar({
  uptime,
  ok,
}: {
  uptime: string;
  ok: boolean;
}) {
  // Neutral default so server HTML and first client render match (no hydration warning).
  const [greet, setGreet] = useState("你好");

  useEffect(() => {
    // Visitor's LOCAL time zone — resolved on the client only.
    setGreet(greetingFor(new Date().getHours()));
  }, []);

  return (
    <section className="statusbar" aria-label="平台状态">
      <div className="container">
        <span className="status-live">
          <span className="beacon" aria-hidden>
            <i />
            <i />
          </span>
          {ok ? "所有系统运行正常" : "部分系统维护中"}
        </span>

        <span className="status-sep" aria-hidden />

        <span className="status-live" style={{ fontWeight: 600 }}>
          可用性 {uptime}
        </span>

        <span className="status-sep" aria-hidden />

        <span className="status-greet">
          <b>{greet}</b>，欢迎回到云枢 👋
        </span>

        <span className="status-guard">
          北京时间 08:00–20:00 工程师在线值守
        </span>
      </div>
    </section>
  );
}
