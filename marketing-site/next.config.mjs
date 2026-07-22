import path from "node:path";
import { fileURLToPath } from "node:url";

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  // 该官网是主仓里的独立 npm 项目。显式限定追踪根，避免 Next 因上层还有
  // package-lock.json 而把整个桌面应用目录纳入服务端文件追踪。
  outputFileTracingRoot: path.dirname(fileURLToPath(import.meta.url)),
};

export default nextConfig;
