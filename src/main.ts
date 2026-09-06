import { createApp } from "vue";
import { createPinia } from "pinia";
import piniaPluginPersistedstate from "pinia-plugin-persistedstate";
import "virtual:uno.css";
import "./assets/styles/calendar-animations.css";
import App from "./App.vue";
import { useSettingStore } from "./stores/settingStore";
import { enableLocalIcons } from "./utils/icons";

const app = createApp(App);
const pinia = createPinia();
pinia.use(piniaPluginPersistedstate);

app.use(pinia);

// 默认本地图标模式：挂载前加载本地图标集，避免首屏图标走 CDN。
// 模式取自前端持久化缓存（同步读取），后端权威值在 App.vue 挂载后再校正。
(async () => {
  const settingStore = useSettingStore(pinia);
  if (settingStore.iconMode === "local") {
    await enableLocalIcons();
  }
  app.mount("#app");
})();
