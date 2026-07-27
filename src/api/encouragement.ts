import { invoke } from "@tauri-apps/api/core";
import type {
  Encouragement,
  AddEncouragementInput,
  UpdateEncouragementInput,
  StreakInfo,
  EncouragementTriggerSource,
  EncouragementSettings,
  UpdateEncouragementSettingsInput,
  SetbackSituation,
} from "@/types";

/** 列出所有鼓励语 */
export async function listEncouragements(): Promise<Encouragement[]> {
  return invoke("list_encouragements");
}

/** 添加自定义鼓励语 */
export async function addEncouragement(
  input: AddEncouragementInput,
): Promise<Encouragement> {
  return invoke("add_encouragement", { input });
}

/** 更新自定义鼓励语（P0-5：预设不允许修改） */
export async function updateEncouragement(
  input: UpdateEncouragementInput,
): Promise<Encouragement> {
  return invoke("update_encouragement", { input });
}

/** 删除鼓励语（预设不允许删除） */
export async function deleteEncouragement(id: string): Promise<void> {
  return invoke("delete_encouragement", { id });
}

/** 随机抽取一句鼓励语（P0-4：含展示去重，triggerSource 必填） */
export async function randomEncouragement(
  triggerSource: EncouragementTriggerSource,
): Promise<Encouragement | null> {
  return invoke("random_encouragement", { triggerSource });
}

/** 根据连续天数智能选择鼓励语等级（1天普通/3天进阶/7天高亮） */
export async function randomEncouragementByStreak(
  streak: number,
  triggerSource: EncouragementTriggerSource,
): Promise<Encouragement | null> {
  return invoke("random_encouragement_by_streak", { streak, triggerSource });
}

/** 抽取庆祝鼓励语（全部目标完成时使用） */
export async function randomCelebrationEncouragement(
  triggerSource: EncouragementTriggerSource,
): Promise<Encouragement | null> {
  return invoke("random_celebration_encouragement", { triggerSource });
}

/** 获取连续完成天数统计 */
export async function getStreak(): Promise<StreakInfo> {
  return invoke("get_streak");
}

// ============================================================
// P1-4: 鼓励语偏好设置
// ============================================================

/** 获取鼓励语偏好设置 */
export async function getEncouragementSettings(): Promise<EncouragementSettings> {
  return invoke("get_encouragement_settings");
}

/** 更新鼓励语偏好设置 */
export async function updateEncouragementSettings(
  input: UpdateEncouragementSettingsInput,
): Promise<void> {
  return invoke("update_encouragement_settings", { input });
}

/** 检测挫折场景（P1-2） */
export async function getSetbackSituation(): Promise<SetbackSituation> {
  return invoke("get_setback_situation");
}

/** 批量删除鼓励语（P1-5） */
export async function batchDeleteEncouragements(
  ids: string[],
): Promise<number> {
  return invoke("batch_delete_encouragements", { ids });
}

/** 批量修改鼓励语等级（P1-5） */
export async function batchUpdateEncouragementLevel(
  ids: string[],
  level: string,
): Promise<number> {
  return invoke("batch_update_encouragement_level", { ids, level });
}

/** 隐藏预设鼓励语（P2-5） */
export async function hidePresetEncouragement(id: string): Promise<void> {
  return invoke("hide_preset_encouragement", { id });
}

/** 重置所有隐藏的预设文案（P2-5） */
export async function resetHiddenPresets(): Promise<number> {
  return invoke("reset_hidden_presets");
}

// ============================================================
// P3-2: 用户收藏机制
// ============================================================

/** 切换收藏状态（已收藏则取消，未收藏则添加） */
export async function toggleFavorite(id: string): Promise<boolean> {
  return invoke("toggle_favorite", { id });
}

/** 获取收藏的鼓励语列表 */
export async function getFavorites(): Promise<Encouragement[]> {
  return invoke("get_favorites");
}

/** 检查鼓励语是否已收藏 */
export async function isFavorite(id: string): Promise<boolean> {
  return invoke("is_favorite", { id });
}

// ============================================================
// P3-3: 展示反馈学习
// ============================================================

/** 记录用户关闭鼓励语弹窗的行为 */
export async function logEncouragementClose(
  id: string,
  viewDuration: number,
): Promise<void> {
  return invoke("log_encouragement_close", { id, viewDuration });
}

/** 鼓励语展示统计 */
export interface EncouragementStats {
  total_shows: number;
  avg_duration: number;
  last_shown: string | null;
}

/** 获取鼓励语展示统计 */
export async function getEncouragementStats(
  id: string,
): Promise<EncouragementStats> {
  return invoke("get_encouragement_stats", { id });
}

// ============================================================
// P3-4: longest_streak 信号利用
// ============================================================

/** 连续天数里程碑信息 */
export interface StreakMilestone {
  milestone_type: string;
  current_streak: number;
  longest_streak: number;
}

/** 检测是否接近/超越历史最长连续天数 */
export async function checkLongestStreakMilestone(): Promise<StreakMilestone | null> {
  return invoke("check_longest_streak_milestone");
}

/** 抽取 longest_streak 触发的鼓励语 */
export async function randomLongestStreakEncouragement(
  triggerSource: EncouragementTriggerSource,
): Promise<Encouragement | null> {
  return invoke("random_longest_streak_encouragement", { triggerSource });
}

// ============================================================
// P3-5: 拖拽排序与自定义顺序
// ============================================================

/** 排序项 */
export interface EncouragementOrderItem {
  id: string;
  sort_order: number;
}

/** 更新鼓励语排序顺序（批量） */
export async function updateEncouragementOrder(
  items: EncouragementOrderItem[],
): Promise<void> {
  return invoke("update_encouragement_order", { items });
}
