import { defineStore } from "pinia";
import { ref, computed } from "vue";
import * as encApi from "@/api/encouragement";
import type {
  Encouragement,
  StreakInfo,
  EncouragementLevel,
  EncouragementTriggerSource,
  EncouragementSettings,
  UpdateEncouragementSettingsInput,
} from "@/types";

/**
 * P0-1 降级方案：后端不可用时使用的应急文案
 * 独立于 DB 预设文案，避免 fallback 命中 DB 文案的违和感
 */
const ENCOURAGEMENT_FALLBACK: string[] = [
  "今天又前进了一步。",
  "完成本身就是奖励。",
  "一步一个脚印，稳稳的。",
  "做到了，就值得肯定。",
  "坚持的人，运气不会差。",
];

/** 鼓励语 Store - 管理鼓励语库和连续天数 */
export const useEncouragementStore = defineStore("encouragement", () => {
  const list = ref<Encouragement[]>([]);
  const streak = ref<StreakInfo>({
    current_streak: 0,
    longest_streak: 0,
    completed_today: false,
    milestone: "none",
  });
  const loaded = ref(false);

  // P1-4：鼓励语偏好设置
  const settings = ref<EncouragementSettings>({
    enabled: true,
    frequency: "normal",
    style: "warm",
    celebration_animation: true,
    emoji_enabled: true,
  });

  const presetList = computed(() =>
    list.value.filter((e) => e.category === "preset"),
  );
  const customList = computed(() =>
    list.value.filter((e) => e.category === "custom"),
  );

  /** 按等级分组 */
  const byLevel = computed(() => {
    const groups: Record<EncouragementLevel, Encouragement[]> = {
      normal: [],
      advanced: [],
      highlight: [],
      celebration: [],
      setback: [],
    };
    for (const e of list.value) {
      if (groups[e.level]) {
        groups[e.level].push(e);
      }
    }
    return groups;
  });

  /** 加载鼓励语列表 */
  async function fetchAll() {
    try {
      list.value = await encApi.listEncouragements();
      loaded.value = true;
    } catch {
      list.value = [];
    }
  }

  /** 加载连续天数 */
  async function fetchStreak() {
    try {
      streak.value = await encApi.getStreak();
    } catch {
      // 忽略
    }
  }

  // P1-4：加载鼓励语偏好设置
  async function fetchSettings() {
    try {
      settings.value = await encApi.getEncouragementSettings();
    } catch {
      // 保持默认值
    }
  }

  // P1-4：更新鼓励语偏好设置
  async function updateSettings(input: UpdateEncouragementSettingsInput) {
    await encApi.updateEncouragementSettings(input);
    // 更新本地状态
    Object.assign(settings.value, input);
  }

  /** 添加自定义鼓励语 */
  async function add(text: string, level: EncouragementLevel = "normal") {
    const item = await encApi.addEncouragement({ text, level });
    list.value.push(item);
    return item;
  }

  /** 更新自定义鼓励语（P0-5：保留 id 与 created_at） */
  async function update(
    id: string,
    text: string,
    level: EncouragementLevel,
  ): Promise<Encouragement> {
    const updated = await encApi.updateEncouragement({ id, text, level });
    const idx = list.value.findIndex((e) => e.id === id);
    if (idx >= 0) {
      list.value.splice(idx, 1, updated);
    }
    return updated;
  }

  /** 删除鼓励语 */
  async function remove(id: string) {
    await encApi.deleteEncouragement(id);
    list.value = list.value.filter((e) => e.id !== id);
  }

  /** 从 fallback 数组随机取一条，包装为 Encouragement 对象 */
  function pickFallback(): Encouragement {
    const idx = Math.floor(Math.random() * ENCOURAGEMENT_FALLBACK.length);
    return {
      id: "fallback",
      text: ENCOURAGEMENT_FALLBACK[idx],
      category: "custom",
      level: "normal",
      created_at: "",
    };
  }

  /** 随机抽取一句鼓励语（全等级，P0-4 含展示去重） */
  async function random(
    triggerSource: EncouragementTriggerSource = "complete_normal",
  ): Promise<Encouragement | null> {
    if (list.value.length === 0) {
      await fetchAll();
    }
    try {
      const enc = await encApi.randomEncouragement(triggerSource);
      if (enc) return enc;
      // 后端返回 null（空库）→ 用 fallback
    } catch {
      // 后端不可用 → 用 fallback
    }
    return pickFallback();
  }

  /** 根据连续天数智能抽取鼓励语（Sprint 5 个性化规则 + P0-4 去重） */
  async function randomByStreak(
    streakDays: number,
    triggerSource: EncouragementTriggerSource = "complete_first",
  ): Promise<Encouragement | null> {
    try {
      const enc = await encApi.randomEncouragementByStreak(
        streakDays,
        triggerSource,
      );
      if (enc) return enc;
      return pickFallback();
    } catch {
      // 后端不可用时降级为全等级随机
      return random(triggerSource);
    }
  }

  /** 抽取庆祝鼓励语（全部目标完成 + P0-4 去重） */
  async function randomCelebration(
    triggerSource: EncouragementTriggerSource = "complete_celebration",
  ): Promise<Encouragement | null> {
    try {
      const enc = await encApi.randomCelebrationEncouragement(triggerSource);
      if (enc) return enc;
      return pickFallback();
    } catch {
      return random(triggerSource);
    }
  }

  // P3-2：收藏状态管理
  const favorites = ref<Set<string>>(new Set());

  /** 加载收藏列表 */
  async function fetchFavorites() {
    try {
      const list = await encApi.getFavorites();
      favorites.value = new Set(list.map((e) => e.id));
    } catch {
      favorites.value.clear();
    }
  }

  /** 切换收藏状态 */
  async function toggleFavoriteStatus(id: string): Promise<boolean> {
    try {
      const isFav = await encApi.toggleFavorite(id);
      if (isFav) {
        favorites.value.add(id);
      } else {
        favorites.value.delete(id);
      }
      return isFav;
    } catch (e) {
      throw e;
    }
  }

  /** 检查是否已收藏 */
  function isFavorited(id: string): boolean {
    return favorites.value.has(id);
  }

  return {
    list,
    streak,
    loaded,
    settings,
    presetList,
    customList,
    byLevel,
    favorites,
    fetchAll,
    fetchStreak,
    fetchSettings,
    add,
    update,
    updateSettings,
    remove,
    random,
    randomByStreak,
    randomCelebration,
    fetchFavorites,
    toggleFavoriteStatus,
    isFavorited,
  };
});
