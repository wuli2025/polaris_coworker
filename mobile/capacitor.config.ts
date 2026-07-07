import type { CapacitorConfig } from "@capacitor/cli";

/**
 * Capacitor 安卓壳配置。
 * webDir 指向 vite 产物;App 装的是这套静态前端,运行时通过用户配置的
 * 远端 base(net.ts)连中控平台 —— 真"空壳",不打包任何后端。
 *
 * 明文 HTTP:局域网主机常是 http://IP:端口,故允许明文(见 android 网络安全配置)。
 */
const config: CapacitorConfig = {
  appId: "com.polaris.mobile",
  appName: "北极星",
  webDir: "dist",
  android: {
    allowMixedContent: true,
  },
  server: {
    androidScheme: "https",
    cleartext: true,
  },
};

export default config;
