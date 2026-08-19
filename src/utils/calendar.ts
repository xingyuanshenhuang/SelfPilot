/**
 * 日历相关的共享常量与工具函数（R-04b）
 *
 * 集中封装日历负载等级、颜色、圆环等展示逻辑与常量，
 * 避免跨组件重复定义，保证一致性。
 */

import type { TaskStatus } from "@/types";

/** 负载等级（"none" 表示无任务） */
export type LoadLevel = "none" | "low" | "medium" | "high" | "extreme";

/** 负载等级阈值 */
export const LOAD_THRESHOLD_MEDIUM = 4;
export const LOAD_THRESHOLD_HIGH = 7;
export const LOAD_THRESHOLD_EXTREME = 11;
/** 单日任务容量上限（用于负载百分比计算） */
export const LOAD_MAX_CAPACITY = 12;
/** 负载圆环半径 */
export const RING_RADIUS = 10;
/** 负载圆环周长 */
export const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

/** 负载等级对应的颜色 */
export const LOAD_COLORS: Record<"low" | "medium" | "high" | "extreme", string> =
  {
    low: "#22c55e",
    medium: "#f59e0b",
    high: "#ef4444",
    extreme: "#9333ea",
  };

/** 根据任务总数计算负载等级 */
export function getLoadLevel(totalTasks: number): LoadLevel {
  if (totalTasks <= 0) return "none";
  if (totalTasks >= LOAD_THRESHOLD_EXTREME) return "extreme";
  if (totalTasks >= LOAD_THRESHOLD_HIGH) return "high";
  if (totalTasks >= LOAD_THRESHOLD_MEDIUM) return "medium";
  return "low";
}

/** 根据负载等级获取颜色 */
export function getLoadColor(level: LoadLevel): string {
  if (level === "none") return "#d1d5db";
  return LOAD_COLORS[level];
}

/** 计算负载百分比（0-100，超过容量上限按 100 截断） */
export function getLoadPercentage(count: number): number {
  return Math.min(100, Math.round((count / LOAD_MAX_CAPACITY) * 100));
}

/** 生成负载圆环的 stroke-dasharray */
export function getRingDashArray(count: number): string {
  const pct = getLoadPercentage(count);
  const filled = (pct / 100) * RING_CIRCUMFERENCE;
  return `${filled} ${RING_CIRCUMFERENCE}`;
}

/** 星期表头（周一起始） */
export const weekDays = ["一", "二", "三", "四", "五", "六", "日"];

/** 任务状态筛选选项 */
export const statusOptions: { label: string; value: TaskStatus }[] = [
  { label: "未完成", value: "pending" },
  { label: "部分完成", value: "partial" },
  { label: "已完成", value: "done" },
  { label: "已跳过", value: "skipped" },
];
