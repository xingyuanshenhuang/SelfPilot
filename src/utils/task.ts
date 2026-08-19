/**
 * 任务相关的共享工具函数（R-04a）
 *
 * 集中封装日历组件中重复使用的任务展示逻辑，
 * 保证 ARIA 标签、阻塞提示等文案的一致性与可维护性。
 */

import { format } from "date-fns";
import { zhCN } from "date-fns/locale";
import type { CalendarTask } from "@/types";
import { STATUS_META } from "@/types";

/** 任务统计（数量维度） */
export interface DayStats {
  total: number;
  done: number;
  partial: number;
  overdue: number;
}

/** 空任务统计（避免重复构造） */
export const EMPTY_DAY_STATS: DayStats = {
  total: 0,
  done: 0,
  partial: 0,
  overdue: 0,
};

/** 计算任务列表的完成统计 */
export function getDayStats(tasks: CalendarTask[]): DayStats {
  const total = tasks.length;
  const done = tasks.filter((t) => t.status === "done").length;
  const partial = tasks.filter((t) => t.status === "partial").length;
  const overdue = tasks.filter((t) => t.is_overdue).length;
  return { total, done, partial, overdue };
}

/** 统一的阻塞提示文案（DUP-F-07 收敛） */
export function getBlockedTooltip(t: {
  blocked_by_names: string | null;
}): string {
  return t.blocked_by_names
    ? `前置未完成：${t.blocked_by_names}`
    : "前置任务未完成";
}

/** 生成任务的无障碍标签 */
export function getTaskAriaLabel(t: CalendarTask): string {
  const parts = [
    t.name,
    `状态：${STATUS_META[t.status].label}`,
    `目标：${t.goal_name}`,
    `进度：${t.actual_qty}/${t.plan_qty}${t.unit}`,
  ];
  if (t.is_overdue) parts.push("已逾期");
  if (t.is_blocked) parts.push(getBlockedTooltip(t));
  return parts.join("，");
}

/** 生成日期单元格的无障碍标签 */
export function getDayAriaLabel(day: Date, stats: DayStats): string {
  const dateStr = format(day, "yyyy 年 M 月 d 日 EEEE", { locale: zhCN });
  if (stats.total === 0) return `${dateStr}，无任务`;
  const parts = [
    `${dateStr}，共 ${stats.total} 个任务`,
    `已完成 ${stats.done}`,
  ];
  if (stats.overdue > 0) parts.push(`${stats.overdue} 个逾期`);
  return parts.join("，");
}
