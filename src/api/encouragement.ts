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
  UserFavorite,
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
  longestStreak: number,
  triggerSource: EncouragementTriggerSource,
): Promise<Encouragement | null> {
  return invoke("random_encouragement_by_streak", {
    streak,
    longestStreak,
    triggerSource,
  });
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

/** 添加收藏 */
export async function addFavorite(
  encouragementId: string,
): Promise<UserFavorite> {
  return invoke("add_favorite", { encouragementId });
}

/** 移除收藏 */
export async function removeFavorite(encouragementId: string): Promise<void> {
  return invoke("remove_favorite", { encouragementId });
}

/** 列出所有收藏 */
export async function listFavorites(): Promise<UserFavorite[]> {
  return invoke("list_favorites");
}

// ============================================================
// P3-3: 展示反馈学习
// ============================================================

/** 记录反馈 */
export async function recordFeedback(
  encouragementId: string,
  feedbackType: "like" | "dislike",
): Promise<void> {
  return invoke("record_feedback", { encouragementId, feedbackType });
}

/** 获取反馈统计 */
export async function getFeedbackStats(): Promise<
  Record<string, { likes: number; dislikes: number }>
> {
  return invoke("get_feedback_stats");
}

// ============================================================
// P3-5: 拖拽排序
// ============================================================

/** 更新鼓励语排序 */
export async function updateEncouragementOrder(
  id: string,
  sortOrder: number,
): Promise<void> {
  return invoke("update_encouragement_order", { id, sortOrder });
}

/** 批量更新排序 */
export async function batchUpdateEncouragementOrder(
  orders: Array<[string, number]>,
): Promise<void> {
  return invoke("batch_update_encouragement_order", { orders });
}

// ============================================================
// P3-6：独立导入导出
// ============================================================

/** 导出鼓励语（JSON格式） */
export async function exportEncouragements(): Promise<string> {
  return invoke("export_encouragements");
}

/** 导入鼓励语（JSON格式） */
export async function importEncouragements(
  json: string,
): Promise<{ imported: number; skipped: number }> {
  const [imported, skipped] = await invoke<[number, number]>(
    "import_encouragements",
    { json },
  );
  return { imported, skipped };
}
