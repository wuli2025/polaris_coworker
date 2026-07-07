import type { Metadata, Viewport } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "云枢 NovaCloud — 面向生产的 AI 云平台",
  description:
    "毫秒级推理、弹性算力、企业级安全。一套统一 API 接入全部主流大模型，已服务 12,800+ 团队。",
  metadataBase: new URL("https://example.com"),
  openGraph: {
    title: "云枢 NovaCloud — 面向生产的 AI 云平台",
    description: "一套统一 API 接入全部主流大模型，已服务 12,800+ 团队。",
    type: "website",
  },
};

export const viewport: Viewport = {
  themeColor: "#0b1020",
  width: "device-width",
  initialScale: 1,
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="zh-CN">
      <body>{children}</body>
    </html>
  );
}
