import { createApp } from "vue";
import App from "./App.vue";
import "./style.css";
import { initViewport } from "./lib/viewport";

// 键盘/安全区适配要在首帧前装好,否则第一次弹键盘会漏掉一次 inset 事件。
initViewport();

createApp(App).mount("#app");
