"use client";

import { useEffect, useRef, type ReactNode } from "react";

/**
 * Wraps children and fades/slides them in when they enter the viewport.
 * Progressive enhancement: content is visible even if JS never runs
 * (the `.reveal` base is only applied once mounted client-side).
 */
export default function Reveal({
  children,
  delay = 0,
  as: Tag = "div",
  className = "",
}: {
  children: ReactNode;
  delay?: number;
  as?: keyof JSX.IntrinsicElements;
  className?: string;
}) {
  const ref = useRef<HTMLElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.classList.add("reveal");
    const io = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          setTimeout(() => el.classList.add("in"), delay);
          io.disconnect();
        }
      },
      { threshold: 0.15 }
    );
    io.observe(el);
    return () => io.disconnect();
  }, [delay]);

  const Component = Tag as any;
  return (
    <Component ref={ref} className={className}>
      {children}
    </Component>
  );
}
