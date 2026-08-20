import { defineStore } from "pinia";
import { ref, computed } from "vue";
import * as settingsApi from "@/api/settings";

/** 设置 Store - 管理主题等应用级设置 */
export const useSettingStore = defineStore(
  "setting",
  () => {
    /** 主题：light | dark */
    const theme = ref<"light" | "dark">("light");
    /** 是否已从后端加载过设置 */
    const loaded = ref(false);

    const isDark = computed(() => theme.value === "dark");

    /** 从后端加载主题设置 */
    async function loadTheme() {
      try {
        const value = await settingsApi.getSetting("theme");
        if (value === "dark" || value === "light") {
          theme.value = value;
        }
        loaded.value = true;
      } catch {
        // 后端未就绪时使用默认值
        theme.value = "light";
      }
    }

    /** 切换主题并持久化到后端 */
    async function setTheme(value: "light" | "dark") {
      theme.value = value;
      try {
        await settingsApi.setSetting({ key: "theme", value });
      } catch {
        // 持久化失败时仍保留前端状态
      }
    }

    /** 切换深浅主题 */
    async function toggleTheme() {
      await setTheme(theme.value === "light" ? "dark" : "light");
    }

    return {
      theme,
      loaded,
      isDark,
      loadTheme,
      setTheme,
      toggleTheme,
    };
  },
  {
    // 前端持久化作为快速启动缓存，后端为权威源
    persist: {
      key: "selfpilot-settings",
      pick: ["theme"],
    },
  },
);
