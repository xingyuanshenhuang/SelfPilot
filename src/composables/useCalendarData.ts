/**
 * 日历数据加载逻辑封装
 *
 * 提供任务数据加载、请求去重、日期范围计算等功能
 */

import { ref, watch, type Ref } from "vue";
import {
  format,
  startOfMonth,
  endOfMonth,
  startOfWeek,
  endOfWeek,
} from "date-fns";
import * as taskApi from "@/api/task";
import * as statsApi from "@/api/stats";
import type { CalendarTask, DailyLoad } from "@/types";
import type { ViewMode } from "./useCalendarNavigation";

export function useCalendarData(
  viewMode: Ref<ViewMode>,
  currentDate: Ref<Date>,
) {
  // ===== 状态 =====

  const tasks = ref<CalendarTask[]>([]);
  const dailyLoadMap = ref<Record<string, DailyLoad>>({});
  const loading = ref(false);

  // ===== P3-3：请求去重机制 =====

  let loadToken = 0;

  async function loadData() {
    const token = ++loadToken;
    loading.value = true;

    try {
      const { start, end } = getDateRange();
      const startStr = format(start, "yyyy-MM-dd");
      const endStr = format(end, "yyyy-MM-dd");

      // 并行查询任务列表和每日负载
      const [taskList, loadList] = await Promise.all([
        taskApi.listTasksByDateRange(startStr, endStr),
        statsApi.getDailyLoad(startStr, endStr),
      ]);

      // 检查是否为最新请求
      if (token !== loadToken) {
        console.log(
          "丢弃过期响应: token=",
          token,
          "当前token=",
          loadToken,
        );
        return;
      }

      tasks.value = taskList;
      dailyLoadMap.value = Object.fromEntries(
        loadList.map((l) => [l.date, l]),
      );
    } catch (e) {
      // 旧请求的错误也忽略
      if (token !== loadToken) return;

      console.error("加载数据失败:", e);
      throw e;
    } finally {
      // 只有最新请求才更新loading
      if (token === loadToken) {
        loading.value = false;
      }
    }
  }

  // ===== 日期范围计算 =====

  function getDateRange(): { start: Date; end: Date } {
    if (viewMode.value === "month") {
      const start = startOfWeek(startOfMonth(currentDate.value), {
        weekStartsOn: 1,
      });
      const end = endOfWeek(endOfMonth(currentDate.value), { weekStartsOn: 1 });
      return { start, end };
    }

    if (viewMode.value === "week") {
      const start = startOfWeek(currentDate.value, { weekStartsOn: 1 });
      const end = endOfWeek(currentDate.value, { weekStartsOn: 1 });
      return { start, end };
    }

    // day 模式：单日查询
    return { start: currentDate.value, end: currentDate.value };
  }

  // ===== 自动监听视图切换加载数据 =====

  watch([viewMode, currentDate], loadData);

  // ===== 返回 =====

  return {
    // 状态
    tasks,
    dailyLoadMap,
    loading,

    // 数据加载
    loadData,
    getDateRange,
  };
}