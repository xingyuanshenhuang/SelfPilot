import { defineStore } from "pinia";
import { ref, computed } from "vue";
import * as settingsApi from "@/api/settings";
import type { SkipBehaviorMode } from "@/types";

/** 设置 Store - 管理主题等应用级设置 */
export const useSettingStore = defineStore(
  "setting",
  () => {
    /** 主题：light | dark */
    const theme = ref<"light" | "dark">("light");
    /** 是否已从后端加载过设置 */
    const loaded = ref(false);

    /** 跳过任务后的处理模式 */
    const skipBehavior = ref<SkipBehaviorMode>("mark_skipped");

    const isDark = computed(() => theme.value === "dark");

    /** 从后端加载主题设置 */
    async function loadTheme() {
      try {
        const value = await settingsApi.getSetting("theme");
        if (value === "dark" || value === "light") {
          theme.value = value;
        }

        // 加载行为设置
        const behavior = await settingsApi.getSetting("skip_behavior");
        if (
          behavior === "mark_skipped" ||
          behavior === "redistribute" ||
          behavior === "auto_extend_deadline"
        ) {
          skipBehavior.value = behavior;
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

    /** 设置跳过行为模式 */
    async function setSkipBehavior(mode: SkipBehaviorMode) {
      skipBehavior.value = mode;
      try {
        await settingsApi.setSetting({ key: "skip_behavior", value: mode });
      } catch {
        // 持久化失败时仍保留前端状态
      }
    }

    return {
      theme,
      loaded,
      skipBehavior,
      isDark,
      loadTheme,
      setTheme,
      toggleTheme,
      setSkipBehavior,
    };
  },
  {
    // 前端持久化作为快速启动缓存，后端为权威源
    persist: {
      key: "selfpilot-settings",
      paths: ["theme", "skipBehavior"],
    },
  },
);
