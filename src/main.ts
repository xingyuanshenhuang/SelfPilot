import { createApp } from "vue";
import { createPinia } from "pinia";
import piniaPluginPersistedstate from "pinia-plugin-persistedstate";
import "virtual:uno.css";
import "./assets/styles/calendar-animations.css";
import App from "./App.vue";
import { initIconRuntime, enableLocalIcons } from "./utils/icons";

const app = createApp(App);
const pinia = createPinia();
pinia.use(piniaPluginPersistedstate);

app.use(pinia);

// 无条件注册本地图标集：内置图标离线可用，任何模式都不依赖 CDN；
// 同时固定 Iconify 运行时 API 端点到 CSP 白名单域名，避免随机镜像触发 CSP 拦截。
(async () => {
  initIconRuntime();
  await enableLocalIcons();
  app.mount("#app");
})();
