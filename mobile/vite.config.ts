import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// 手机壳前端:纯静态,打进 dist 后由 Capacitor 装进 WebView。
// 所有后端请求走用户配置的远端 base(见 src/lib/net.ts),不依赖同源。
export default defineConfig({
  plugins: [vue()],
  base: "./",
  server: { port: 1431, host: true },
  build: { outDir: "dist", target: "es2020", sourcemap: false },
});
