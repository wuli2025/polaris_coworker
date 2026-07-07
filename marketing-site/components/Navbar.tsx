"use client";

import { useEffect, useState } from "react";

const LINKS = [
  { href: "#features", label: "功能特性" },
  { href: "#customers", label: "客户案例" },
  { href: "#pricing", label: "定价" },
  { href: "#faq", label: "常见问题" },
];

export default function Navbar() {
  const [scrolled, setScrolled] = useState(false);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 8);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  return (
    <>
      <header className={`nav${scrolled ? " scrolled" : ""}`}>
        <div className="container">
          <a href="#top" className="brand" aria-label="云枢 NovaCloud 首页">
            <span className="dot" aria-hidden />
            云枢<span className="en">NovaCloud</span>
          </a>

          <nav className="nav-links" aria-label="主导航">
            {LINKS.map((l) => (
              <a key={l.href} href={l.href}>
                {l.label}
              </a>
            ))}
          </nav>

          <div className="nav-cta">
            <a href="#" className="nav-signin">
              登录
            </a>
            <a href="#pricing" className="btn btn-primary">
              免费试用
            </a>
          </div>

          <button
            className="nav-toggle"
            aria-label="切换菜单"
            aria-expanded={open}
            onClick={() => setOpen((v) => !v)}
          >
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none">
              {open ? (
                <path
                  d="M6 6l12 12M18 6L6 18"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                />
              ) : (
                <path
                  d="M4 7h16M4 12h16M4 17h16"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                />
              )}
            </svg>
          </button>
        </div>
      </header>

      {open && (
        <div className="nav-mobile">
          {LINKS.map((l) => (
            <a key={l.href} href={l.href} onClick={() => setOpen(false)}>
              {l.label}
            </a>
          ))}
          <a href="#" onClick={() => setOpen(false)}>
            登录
          </a>
          <a
            href="#pricing"
            className="btn btn-primary"
            onClick={() => setOpen(false)}
          >
            免费试用
          </a>
        </div>
      )}
    </>
  );
}
