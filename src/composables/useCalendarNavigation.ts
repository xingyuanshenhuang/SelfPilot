/**
 * 日历导航逻辑封装
 *
 * 提供日期导航、视图切换、标题计算等功能
 */

import { ref, computed } from "vue";
import {
  format,
  addMonths,
  subMonths,
  addWeeks,
  subWeeks,
  addDays,
  subDays,
} from "date-fns";
import { zhCN } from "date-fns/locale";

export type ViewMode = "month" | "week" | "day";

export function useCalendarNavigation(initialMode: ViewMode = "month") {
  // ===== 状态 =====

  const viewMode = ref<ViewMode>(initialMode);
  const currentDate = ref(new Date());
  const selectedDate = ref(new Date());
  const prevViewMode = ref<ViewMode | null>(null);

  // ===== 导航函数 =====

  function prev() {
    if (viewMode.value === "month") {
      currentDate.value = subMonths(currentDate.value, 1);
    } else if (viewMode.value === "week") {
      currentDate.value = subWeeks(currentDate.value, 1);
    } else {
      currentDate.value = subDays(currentDate.value, 1);
    }
  }

  function next() {
    if (viewMode.value === "month") {
      currentDate.value = addMonths(currentDate.value, 1);
    } else if (viewMode.value === "week") {
      currentDate.value = addWeeks(currentDate.value, 1);
    } else {
      currentDate.value = addDays(currentDate.value, 1);
    }
  }

  function goToday() {
    currentDate.value = new Date();
    selectedDate.value = new Date();
  }

  function selectDay(day: Date) {
    selectedDate.value = day;
  }

  function syncFocusedDay() {
    // 用于键盘导航时同步焦点日期
    selectedDate.value = new Date(currentDate.value);
  }

  // ===== 计算属性 =====

  const headerLabel = computed(() => {
    if (viewMode.value === "month") {
      return format(currentDate.value, "yyyy 年 M 月", { locale: zhCN });
    } else if (viewMode.value === "week") {
      return format(currentDate.value, "yyyy 年 M 月 第 W 周", { locale: zhCN });
    } else {
      return format(currentDate.value, "yyyy-MM-dd EEEE", { locale: zhCN });
    }
  });

  const periodLabel = computed(() => {
    if (viewMode.value === "month") {
      return "本月";
    } else if (viewMode.value === "week") {
      return "本周";
    } else {
      return "今日";
    }
  });

  // ===== 视图切换 =====

  function switchView(newMode: ViewMode) {
    if (viewMode.value !== newMode) {
      // 记录上一个视图,用于 Esc 返回
      if (viewMode.value === "month" && newMode !== "month") {
        prevViewMode.value = "month";
      } else {
        prevViewMode.value = null;
      }
      viewMode.value = newMode;
    }
  }

  // ===== 返回 =====

  return {
    // 状态
    viewMode,
    currentDate,
    selectedDate,
    prevViewMode,

    // 导航
    prev,
    next,
    goToday,
    selectDay,
    syncFocusedDay,

    // 计算属性
    headerLabel,
    periodLabel,

    // 视图切换
    switchView,
  };
}