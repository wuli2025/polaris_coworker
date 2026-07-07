"use client";

import { useState } from "react";
import { faqs } from "@/lib/data";

export default function Faq() {
  const [open, setOpen] = useState<number | null>(0);

  return (
    <section id="faq" className="section">
      <div className="container">
        <div className="section-head">
          <span className="eyebrow">常见问题</span>
          <h2>还有疑问？</h2>
          <p>这里是团队最常问到的问题；如未涵盖，欢迎随时联系我们。</p>
        </div>

        <div className="faq-wrap">
          {faqs.map((item, i) => {
            const isOpen = open === i;
            return (
              <div key={item.q} className={`faq-item${isOpen ? " open" : ""}`}>
                <button
                  className="faq-q"
                  aria-expanded={isOpen}
                  aria-controls={`faq-a-${i}`}
                  onClick={() => setOpen(isOpen ? null : i)}
                >
                  {item.q}
                  <svg className="chev" viewBox="0 0 24 24" aria-hidden>
                    <path d="M12 15.4 6.6 10l1.4-1.4 4 4 4-4L17.4 10 12 15.4Z" />
                  </svg>
                </button>
                <div className="faq-a" id={`faq-a-${i}`} role="region">
                  <div>
                    <p>{item.a}</p>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
