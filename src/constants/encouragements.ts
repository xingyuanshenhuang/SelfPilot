/**
 * @deprecated P0-1：鼓励语源已统一到 DB，本文件不再被任何代码引用。
 * 保留导出仅供回滚参考，randomEncouragement() 调用会抛错以暴露残留引用。
 * 应急文案请使用 encouragementStore 内联的 ENCOURAGEMENT_FALLBACK。
 */
export const DEFAULT_ENCOURAGEMENTS: string[] = [
  "今天又进步了！",
  "坚持就是胜利，继续加油！",
  "每一步都算数，你做得很棒！",
  "学习是给自己最好的礼物。",
  "今天的努力，是明天的底气。",
  "小步快跑，日积月累就是大跨越！",
  "你比昨天的自己更强了。",
  "完成一个任务就是一次胜利！",
  "自律给你自由，继续前行。",
  "种一棵树最好的时间是十年前，其次是现在。",
];

/**
 * @deprecated 已废弃，请使用 encouragementStore.random()。
 * 调用会抛错以暴露未迁移的残留引用。
 */
export function randomEncouragement(): string {
  throw new Error(
    "randomEncouragement() 已废弃，请使用 encouragementStore.random()",
  );
}
