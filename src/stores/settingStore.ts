import { defineStore } from "pinia";
import { ref, computed } from "vue";
import * as settingsApi from "@/api/settings";
import { enableLocalIcons, type IconMode } from "@/utils/icons";

/** 设置 Store - 管理主题等应用级设置 */
export const useSettingStore = defineStore(
  "setting",
  () => {
    /** 主题：light | dark */
    const theme = ref<"light" | "dark">("light");
    /** 图标加载模式：local = 本地内置（默认，离线可用）；online = 联网按需拉取（备用） */
    const iconMode = ref<IconMode>("local");
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

    /** 从后端加载图标模式设置（后端为权威源） */
    async function loadIconMode() {
      try {
        const value = await settingsApi.getSetting("icon_mode");
        if (value === "local" || value === "online") {
          iconMode.value = value;
        }
        if (iconMode.value === "local") {
          await enableLocalIcons();
        }
      } catch {
        // 后端未就绪时保持当前值（默认 local）
      }
    }

    /** 切换图标模式并持久化到后端；切到 local 时立即加载本地图标集 */
    async function setIconMode(value: IconMode) {
      iconMode.value = value;
      try {
        await settingsApi.setSetting({ key: "icon_mode", value });
      } catch {
        // 持久化失败时仍保留前端状态
      }
      if (value === "local") {
        await enableLocalIcons();
      }
    }

    return {
      theme,
      iconMode,
      loaded,
      isDark,
      loadTheme,
      setTheme,
      toggleTheme,
      loadIconMode,
      setIconMode,
    };
  },
  {
    // 前端持久化作为快速启动缓存，后端为权威源
    persist: {
      key: "selfpilot-settings",
      pick: ["theme", "iconMode"],
    },
  },
);
