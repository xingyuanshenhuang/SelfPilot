import { invoke } from "@tauri-apps/api/core";
import type {
  DailyTrend,
  GoalCompletionStat,
  HeatmapCell,
  CompletionPrediction,
  DailyLoad,
  CelebrationAchievement,
} from "@/types";

/** 获取近 N 天每日完成趋势 */
export async function getCompletionTrend(days: number): Promise<DailyTrend[]> {
  return invoke("get_completion_trend", { days });
}

/** 获取所有目标的完成统计（柱状图） */
export async function getGoalCompletionStats(): Promise<GoalCompletionStat[]> {
  return invoke("get_goal_completion_stats");
}

/** 获取日历热力图数据 */
export async function getHeatmap(
  startDate: string,
  endDate: string,
): Promise<HeatmapCell[]> {
  return invoke("get_heatmap", { startDate, endDate });
}

/** 获取所有目标的完成预测（基于过去7天平均速度） */
export async function getCompletionPredictions(): Promise<
  CompletionPrediction[]
> {
  return invoke("get_completion_predictions");
}

/** 获取日期范围内每日负载（按目标分组）— P2-5 */
export async function getDailyLoad(
  startDate: string,
  endDate: string,
): Promise<DailyLoad[]> {
  return invoke("get_daily_load", { startDate, endDate });
}

/** 获取庆祝成就数据（P1-3） */
export async function getCelebrationAchievement(): Promise<CelebrationAchievement> {
  return invoke("get_celebration_achievement");
}
